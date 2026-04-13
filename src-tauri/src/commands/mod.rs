pub mod featured_marketplaces;
pub mod plugins;
pub mod security;

use crate::models::{FeaturedRepositoriesConfig, Repository, Skill};
use crate::services::{Database, GitHubService, PluginManager, SkillManager};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tauri::State;
use tokio::sync::Mutex;

/// 扫描进度事件（security 和 plugins 共用）
#[derive(serde::Serialize, Clone)]
pub struct ScanProgressEvent {
    pub scan_id: String,
    pub kind: String,
    pub item_id: String,
    pub file_path: String,
}

/// 默认扫描并行度
pub const DEFAULT_SCAN_PARALLELISM: usize = 3;
/// 最大扫描并行度
pub const MAX_SCAN_PARALLELISM: usize = 8;

pub fn clamp_scan_parallelism(scan_parallelism: Option<usize>) -> usize {
    scan_parallelism
        .unwrap_or(DEFAULT_SCAN_PARALLELISM)
        .clamp(1, MAX_SCAN_PARALLELISM)
}

pub struct AppState {
    pub db: Arc<Database>,
    pub skill_manager: Arc<Mutex<SkillManager>>,
    pub plugin_manager: Arc<Mutex<PluginManager>>,
    pub github: Arc<GitHubService>,
}

fn merge_scanned_skill(existing: Option<&Skill>, scanned: Skill) -> Skill {
    let Some(existing) = existing else {
        return scanned;
    };

    let mut merged = scanned;
    merged.version = existing.version.clone();
    merged.author = existing.author.clone();
    merged.installed = existing.installed;
    merged.installed_at = existing.installed_at;
    merged.local_path = existing.local_path.clone();
    merged.local_paths = existing.local_paths.clone();
    merged.security_score = existing.security_score;
    merged.security_issues = existing.security_issues.clone();
    merged.security_level = existing.security_level.clone();
    merged.security_report = existing.security_report.clone();
    merged.scanned_at = existing.scanned_at;
    merged.installed_commit_sha = existing.installed_commit_sha.clone();
    merged
}

fn collect_stale_uninstalled_skill_ids(
    existing_skills: &[Skill],
    fresh_skills: &[Skill],
) -> Vec<String> {
    let fresh_ids: HashSet<&str> = fresh_skills.iter().map(|skill| skill.id.as_str()).collect();

    existing_skills
        .iter()
        .filter(|skill| !skill.installed && !fresh_ids.contains(skill.id.as_str()))
        .map(|skill| skill.id.clone())
        .collect()
}

/// 添加仓库
#[tauri::command]
pub async fn add_repository(
    state: State<'_, AppState>,
    url: String,
    name: String,
) -> Result<String, String> {
    let repo = Repository::new(url, name);
    let repo_id = repo.id.clone();
    state.db.add_repository(&repo).map_err(|e| e.to_string())?;
    Ok(repo_id)
}

/// 获取所有仓库
#[tauri::command]
pub async fn get_repositories(state: State<'_, AppState>) -> Result<Vec<Repository>, String> {
    state.db.get_repositories().map_err(|e| e.to_string())
}

/// 删除仓库（同时删除未安装的技能和清理缓存）
#[tauri::command]
pub async fn delete_repository(state: State<'_, AppState>, repo_id: String) -> Result<(), String> {
    // 1. 获取仓库信息
    let repo = state
        .db
        .get_repository(&repo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "仓库不存在".to_string())?;

    let repository_url = repo.url.clone();
    let cache_path = repo.cache_path.clone();

    // 2. 删除未安装的技能（使用事务）
    let deleted_skills_count = state
        .db
        .delete_uninstalled_skills_by_repository_url(&repository_url)
        .map_err(|e| e.to_string())?;

    log::info!(
        "删除仓库 {} 的 {} 个未安装技能",
        repo.name,
        deleted_skills_count
    );

    let deleted_plugins_count = state
        .db
        .delete_uninstalled_plugins_by_repository_url(&repository_url)
        .map_err(|e| e.to_string())?;

    log::info!(
        "删除仓库 {} 的 {} 个未安装插件",
        repo.name,
        deleted_plugins_count
    );

    // 3. 清理缓存目录（失败不中断）
    if let Some(cache_path_str) = cache_path {
        let cache_path_buf = std::path::PathBuf::from(&cache_path_str);
        if cache_path_buf.exists() {
            match std::fs::remove_dir_all(&cache_path_buf) {
                Ok(_) => log::info!("成功删除缓存目录: {:?}", cache_path_buf),
                Err(e) => log::warn!(
                    "删除缓存目录失败，但不影响仓库删除: {:?}, 错误: {}",
                    cache_path_buf,
                    e
                ),
            }
        } else {
            log::info!("缓存目录不存在，跳过清理: {:?}", cache_path_buf);
        }
    }

    // 4. 删除仓库记录
    state
        .db
        .delete_repository(&repo_id)
        .map_err(|e| e.to_string())?;

    log::info!("成功删除仓库: {}", repo.name);
    Ok(())
}

/// 扫描仓库中的 skills
#[tauri::command]
pub async fn scan_repository(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<Vec<Skill>, String> {
    use chrono::Utc;

    // 获取仓库信息
    let repo = state
        .db
        .get_repository(&repo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "仓库不存在".to_string())?;

    let (owner, repo_name) = Repository::from_github_url(&repo.url).map_err(|e| e.to_string())?;

    // 确定缓存基础目录
    let cache_base_dir = dirs::cache_dir()
        .ok_or("无法获取缓存目录".to_string())?
        .join("agent-skills-guard")
        .join("repositories");

    let cache_path_for_scan = if let Some(cache_path) = &repo.cache_path {
        // 使用缓存扫描(0次API请求)
        let cache_path_buf = std::path::PathBuf::from(cache_path);
        if cache_path_buf.exists() && cache_path_buf.is_dir() {
            log::info!("使用本地缓存扫描仓库: {}", repo.name);
            cache_path_buf
        } else {
            // 缓存路径不存在，重新下载
            log::warn!("缓存路径不存在，重新下载: {:?}", cache_path_buf);
            let (extract_dir, commit_sha) = state
                .github
                .download_repository_archive(&owner, &repo_name, &cache_base_dir)
                .await
                .map_err(|e| format!("下载仓库压缩包失败: {}", e))?;

            // 更新数据库缓存信息
            state
                .db
                .update_repository_cache(
                    &repo_id,
                    &extract_dir.to_string_lossy(),
                    Utc::now(),
                    Some(&commit_sha),
                )
                .map_err(|e| e.to_string())?;

            extract_dir
        }
    } else {
        // 首次扫描: 下载压缩包并缓存(1次API请求)
        log::info!("首次扫描，下载仓库压缩包: {}", repo.name);

        let (extract_dir, commit_sha) = state
            .github
            .download_repository_archive(&owner, &repo_name, &cache_base_dir)
            .await
            .map_err(|e| format!("下载仓库压缩包失败: {}", e))?;

        // 更新数据库缓存信息
        state
            .db
            .update_repository_cache(
                &repo_id,
                &extract_dir.to_string_lossy(),
                Utc::now(),
                Some(&commit_sha),
            )
            .map_err(|e| e.to_string())?;

        extract_dir
    };

    let skills = state
        .github
        .scan_cached_repository(&cache_path_for_scan, &repo.url, repo.scan_subdirs)
        .map_err(|e| format!("扫描缓存失败: {}", e))?;

    let existing_repo_skills: Vec<Skill> = state
        .db
        .get_skills()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|skill| skill.repository_url == repo.url)
        .collect();
    let existing_by_id: HashMap<String, Skill> = existing_repo_skills
        .iter()
        .cloned()
        .map(|skill| (skill.id.clone(), skill))
        .collect();
    let merged_skills: Vec<Skill> = skills
        .into_iter()
        .filter(|skill| {
            if skill.file_path.trim().is_empty() {
                log::warn!("跳过无效技能记录：名称={}, 路径为空", skill.name);
                false
            } else {
                true
            }
        })
        .map(|skill| merge_scanned_skill(existing_by_id.get(&skill.id), skill))
        .collect();

    // 保存到数据库
    for skill in &merged_skills {
        state.db.save_skill(skill).map_err(|e| e.to_string())?;
    }

    let stale_skill_ids =
        collect_stale_uninstalled_skill_ids(&existing_repo_skills, &merged_skills);
    if !stale_skill_ids.is_empty() {
        let deleted_count = state
            .db
            .delete_skills_by_ids(&stale_skill_ids)
            .map_err(|e| e.to_string())?;
        log::info!(
            "清理仓库 {} 的 {} 个已失效未安装技能",
            repo.name,
            deleted_count
        );
    }

    let deleted_plugins_count = state
        .db
        .delete_uninstalled_plugins_by_repository_url(&repo.url)
        .map_err(|e| e.to_string())?;
    if deleted_plugins_count > 0 {
        log::info!(
            "清理仓库 {} 的 {} 个未安装插件",
            repo.name,
            deleted_plugins_count
        );
    }

    state
        .db
        .set_repository_last_scanned(&repo_id, Utc::now())
        .map_err(|e| e.to_string())?;

    Ok(merged_skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_scanned_skill_preserves_install_state_and_security_metadata() {
        let mut existing = Skill::new(
            "Original".to_string(),
            "https://github.com/example/repo".to_string(),
            "skill-a".to_string(),
        );
        existing.description = Some("existing description".to_string());
        existing.installed = true;
        existing.installed_at = Some(chrono::Utc::now());
        existing.local_path = Some("/tmp/skill-a".to_string());
        existing.local_paths = Some(vec!["/tmp/skill-a".to_string(), "/tmp/skill-b".to_string()]);
        existing.security_score = Some(88);
        existing.security_level = Some("Low".to_string());
        existing.security_issues = Some(vec![crate::models::security::SecurityIssue {
            severity: crate::models::security::IssueSeverity::Warning,
            category: crate::models::security::IssueCategory::Other,
            description: "Warning: existing".to_string(),
            line_number: None,
            code_snippet: None,
            file_path: None,
        }]);
        existing.security_report = Some(crate::models::security::SecurityReport {
            skill_id: existing.id.clone(),
            score: 88,
            level: crate::models::security::SecurityLevel::Low,
            issues: existing.security_issues.clone().unwrap_or_default(),
            recommendations: vec![],
            blocked: false,
            hard_trigger_issues: vec![],
            scanned_files: vec![],
            partial_scan: false,
            skipped_files: vec![],
        });
        existing.installed_commit_sha = Some("abc1234".to_string());
        existing.version = Some("1.0.0".to_string());
        existing.author = Some("Bruce".to_string());

        let mut scanned = Skill::new(
            "Updated".to_string(),
            "https://github.com/example/repo".to_string(),
            "skill-a".to_string(),
        );
        scanned.description = Some("new description".to_string());
        scanned.checksum = Some("new-checksum".to_string());

        let merged = merge_scanned_skill(Some(&existing), scanned);

        assert_eq!(merged.name, "Updated");
        assert_eq!(merged.description.as_deref(), Some("new description"));
        assert_eq!(merged.checksum.as_deref(), Some("new-checksum"));
        assert!(merged.installed);
        assert_eq!(merged.local_path.as_deref(), Some("/tmp/skill-a"));
        assert_eq!(
            merged.local_paths,
            Some(vec!["/tmp/skill-a".to_string(), "/tmp/skill-b".to_string()])
        );
        assert_eq!(merged.security_score, Some(88));
        assert_eq!(merged.security_level.as_deref(), Some("Low"));
        assert!(merged.security_report.is_some());
        assert_eq!(merged.installed_commit_sha.as_deref(), Some("abc1234"));
        assert_eq!(merged.version.as_deref(), Some("1.0.0"));
        assert_eq!(merged.author.as_deref(), Some("Bruce"));
    }

    #[test]
    fn collect_stale_uninstalled_skill_ids_only_returns_missing_uninstalled_records() {
        let mut installed = Skill::new(
            "Installed".to_string(),
            "https://github.com/example/repo".to_string(),
            "installed".to_string(),
        );
        installed.installed = true;

        let stale = Skill::new(
            "Stale".to_string(),
            "https://github.com/example/repo".to_string(),
            "stale".to_string(),
        );
        let fresh = Skill::new(
            "Fresh".to_string(),
            "https://github.com/example/repo".to_string(),
            "fresh".to_string(),
        );

        let stale_ids = collect_stale_uninstalled_skill_ids(
            &[installed.clone(), stale.clone(), fresh.clone()],
            &[installed, fresh],
        );

        assert_eq!(stale_ids, vec![stale.id]);
    }
}

/// 获取所有 skills
#[tauri::command]
pub async fn get_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let manager = state.skill_manager.lock().await;
    manager.get_all_skills().map_err(|e| e.to_string())
}

/// 获取已安装的 skills
#[tauri::command]
pub async fn get_installed_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let manager = state.skill_manager.lock().await;
    manager.get_installed_skills().map_err(|e| e.to_string())
}

/// 安装 skill
#[tauri::command]
pub async fn install_skill(
    state: State<'_, AppState>,
    skill_id: String,
    install_path: Option<String>,
    allow_partial_scan: Option<bool>,
) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .install_skill(&skill_id, install_path, allow_partial_scan.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

/// 准备安装技能：下载并扫描，但不标记为已安装
#[tauri::command]
pub async fn prepare_skill_installation(
    state: State<'_, AppState>,
    skill_id: String,
    locale: String,
) -> Result<crate::models::security::SecurityReport, String> {
    let manager = state.skill_manager.lock().await;
    manager
        .prepare_skill_installation(&skill_id, &locale)
        .await
        .map_err(|e| e.to_string())
}

/// 确认安装技能：标记为已安装
#[tauri::command]
pub async fn confirm_skill_installation(
    state: State<'_, AppState>,
    skill_id: String,
    install_path: Option<String>,
    allow_partial_scan: Option<bool>,
) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .confirm_skill_installation(&skill_id, install_path, allow_partial_scan.unwrap_or(false))
        .map_err(|e| e.to_string())
}

/// 取消安装技能：删除已下载的文件
#[tauri::command]
pub async fn cancel_skill_installation(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .cancel_skill_installation(&skill_id)
        .map_err(|e| e.to_string())
}

/// 卸载 skill
#[tauri::command]
pub async fn uninstall_skill(state: State<'_, AppState>, skill_id: String) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .uninstall_skill(&skill_id)
        .map_err(|e| e.to_string())
}

/// 卸载特定路径的技能
#[tauri::command]
pub async fn uninstall_skill_path(
    state: State<'_, AppState>,
    skill_id: String,
    path: String,
) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .uninstall_skill_path(&skill_id, &path)
        .map_err(|e| e.to_string())
}

/// 删除 skill 记录
#[tauri::command]
pub async fn delete_skill(state: State<'_, AppState>, skill_id: String) -> Result<(), String> {
    state.db.delete_skill(&skill_id).map_err(|e| e.to_string())
}

/// 扫描本地技能目录并导入未追踪的技能
#[tauri::command]
pub async fn scan_local_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let manager = state.skill_manager.lock().await;
    manager.scan_local_skills().map_err(|e| e.to_string())
}

/// 清理指定仓库的缓存
#[tauri::command]
pub async fn clear_repository_cache(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<(), String> {
    let repo = state
        .db
        .get_repository(&repo_id)
        .map_err(|e| e.to_string())?
        .ok_or("仓库不存在")?;

    if let Some(cache_path) = &repo.cache_path {
        let cache_path_buf = std::path::PathBuf::from(cache_path);

        // 验证缓存路径是否在预期的缓存目录中
        let expected_cache_base = dirs::cache_dir()
            .ok_or("无法获取缓存目录".to_string())?
            .join("agent-skills-guard")
            .join("repositories");

        // 删除整个仓库缓存目录（包括archive.zip和extracted/）
        if let Some(parent) = cache_path_buf.parent() {
            // 安全检查：确保路径在预期的缓存目录中
            if !parent.starts_with(&expected_cache_base) {
                return Err("缓存路径无效".to_string());
            }

            // 先清除数据库中的缓存信息
            state
                .db
                .clear_repository_cache_metadata(&repo_id)
                .map_err(|e| e.to_string())?;

            // 然后删除文件（即使失败也不影响数据库一致性）
            if parent.exists() {
                if let Err(e) = std::fs::remove_dir_all(parent) {
                    log::warn!(
                        "删除缓存目录失败，但数据库已清理: {:?}，错误: {}",
                        parent,
                        e
                    );
                    // 不返回错误，因为数据库已经一致
                } else {
                    log::info!("已删除缓存目录: {:?}", parent);
                }
            }
        }
    }

    Ok(())
}

/// 刷新仓库缓存（清理后重新扫描）
#[tauri::command]
pub async fn refresh_repository_cache(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<Vec<Skill>, String> {
    // 先清理缓存
    clear_repository_cache(state.clone(), repo_id.clone()).await?;

    // 重新扫描（会自动下载新版本）
    scan_repository(state, repo_id).await
}

/// 一键清除所有仓库缓存（但保留仓库记录）
#[tauri::command]
pub async fn clear_all_repository_caches(
    state: State<'_, AppState>,
) -> Result<ClearAllCachesResult, String> {
    let repos = state.db.get_repositories().map_err(|e| e.to_string())?;

    let mut cleared_count = 0;
    let mut failed_count = 0;
    let mut total_size_freed: u64 = 0;

    // 获取缓存基础目录
    let cache_base_dir = dirs::cache_dir()
        .ok_or("无法获取缓存目录".to_string())?
        .join("agent-skills-guard")
        .join("repositories");

    if !cache_base_dir.exists() {
        // 缓存目录不存在，无需清理
        return Ok(ClearAllCachesResult {
            total_repositories: repos.len(),
            cleared_count: 0,
            failed_count: 0,
            total_size_freed: 0,
        });
    }

    for repo in &repos {
        if let Some(cache_path) = &repo.cache_path {
            let cache_path_buf = std::path::PathBuf::from(cache_path);

            if let Some(parent) = cache_path_buf.parent() {
                // 安全检查：确保路径在预期的缓存目录中
                if !parent.starts_with(&cache_base_dir) {
                    log::warn!("跳过无效的缓存路径: {:?}", parent);
                    failed_count += 1;
                    continue;
                }

                // 计算目录大小（在删除前）
                if parent.exists() {
                    if let Ok(size) = dir_size(parent) {
                        total_size_freed += size;
                    }
                }

                // 清除数据库中的缓存信息
                if let Err(e) = state.db.clear_repository_cache_metadata(&repo.id) {
                    log::warn!("清除仓库 {} 的缓存元数据失败: {}", repo.name, e);
                    failed_count += 1;
                    continue;
                }

                // 删除文件
                if parent.exists() {
                    if let Err(e) = std::fs::remove_dir_all(parent) {
                        log::warn!("删除缓存目录失败: {:?}，错误: {}", parent, e);
                        failed_count += 1;
                    } else {
                        log::info!("已删除缓存目录: {:?}", parent);
                        cleared_count += 1;
                    }
                } else {
                    // 数据库中有记录但文件不存在，只清理元数据
                    cleared_count += 1;
                }
            }
        }
    }

    log::info!(
        "清除所有缓存完成: 成功 {}, 失败 {}, 释放 {} 字节",
        cleared_count,
        failed_count,
        total_size_freed
    );

    Ok(ClearAllCachesResult {
        total_repositories: repos.len(),
        cleared_count,
        failed_count,
        total_size_freed,
    })
}

/// 获取缓存统计信息
#[tauri::command]
pub async fn get_cache_stats(state: State<'_, AppState>) -> Result<CacheStats, String> {
    let repos = state.db.get_repositories().map_err(|e| e.to_string())?;

    let mut total_cached = 0;
    let mut total_size: u64 = 0;

    for repo in &repos {
        if let Some(cache_path) = &repo.cache_path {
            if let Some(parent) = std::path::PathBuf::from(cache_path).parent() {
                if parent.exists() {
                    total_cached += 1;

                    // 计算目录大小
                    if let Ok(size) = dir_size(parent) {
                        total_size += size;
                    }
                }
            }
        }
    }

    Ok(CacheStats {
        total_repositories: repos.len(),
        cached_repositories: total_cached,
        total_size_bytes: total_size,
    })
}

/// 计算目录大小
fn dir_size(path: &std::path::Path) -> Result<u64, std::io::Error> {
    use walkdir::WalkDir;

    let mut size = 0;

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            size += entry.metadata()?.len();
        }
    }

    Ok(size)
}

/// 缓存统计信息
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub total_repositories: usize,
    pub cached_repositories: usize,
    pub total_size_bytes: u64,
}

/// 清除所有缓存的结果
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearAllCachesResult {
    pub total_repositories: usize,
    pub cleared_count: usize,
    pub failed_count: usize,
    pub total_size_freed: u64,
}

/// 打开技能目录
#[tauri::command]
pub async fn open_skill_directory(
    state: State<'_, AppState>,
    local_path: String,
) -> Result<(), String> {
    use std::process::Command;

    let path = std::path::Path::new(&local_path);

    // 验证路径是否存在且为目录
    if !path.exists() || !path.is_dir() {
        return Err(format!(
            "Path does not exist or is not a directory: {}",
            local_path
        ));
    }

    // 规范化路径，防止路径遍历
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;

    // 验证路径在允许的范围内（技能安装目录或缓存目录）
    let allowed = {
        let mut allowed_paths: Vec<std::path::PathBuf> = Vec::new();

        // 允许的目录：用户 .claude 目录
        if let Some(home) = dirs::home_dir() {
            allowed_paths.push(home.join(".claude"));
        }

        // 允许的目录：应用缓存目录
        if let Some(cache_dir) = dirs::cache_dir() {
            allowed_paths.push(cache_dir.join("agent-skills-guard"));
        }

        // 允许的目录：数据库中记录的已安装技能路径
        if let Ok(skills) = state.db.get_skills() {
            for skill in skills {
                if let Some(paths) = &skill.local_paths {
                    for p in paths {
                        if let Ok(cp) = std::path::Path::new(p).canonicalize() {
                            allowed_paths.push(cp);
                        }
                    }
                }
                if let Some(p) = &skill.local_path {
                    if let Ok(cp) = std::path::Path::new(p).canonicalize() {
                        allowed_paths.push(cp);
                    }
                }
            }
        }

        allowed_paths
            .iter()
            .any(|allowed| canonical.starts_with(allowed))
    };

    if !allowed {
        return Err("Path is not within an allowed directory".to_string());
    }

    let canonical_str = canonical.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&canonical_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&canonical_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&canonical_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    Ok(())
}

/// 获取默认的用户目录安装路径
#[tauri::command]
pub async fn get_default_install_path() -> Result<String, String> {
    let user_path = dirs::home_dir()
        .ok_or("无法获取用户主目录")?
        .join(".claude")
        .join("skills");

    Ok(user_path.to_string_lossy().to_string())
}

/// 打开文件夹选择器，让用户选择自定义安装路径
#[tauri::command]
pub async fn select_custom_install_path(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let folder_path = app
        .dialog()
        .file()
        .set_title("选择技能安装目录")
        .blocking_pick_folder();

    if let Some(file_path) = folder_path {
        // 转换为 PathBuf
        let path = std::path::PathBuf::from(file_path.to_string());

        // 验证路径可写
        let test_file = path.join(".write_test");
        match std::fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = std::fs::remove_file(&test_file);
                Ok(Some(path.to_string_lossy().to_string()))
            }
            Err(_) => Err("选择的目录不可写，请检查权限".to_string()),
        }
    } else {
        Ok(None)
    }
}

const FEATURED_REPOSITORIES_REMOTE_URL: &str =
    "https://raw.githubusercontent.com/bruc3van/agent-skills-guard/main/featured-marketplace.yaml";
const DEFAULT_FEATURED_REPOSITORIES_YAML: &str = include_str!("../../../featured-marketplace.yaml");

fn featured_repositories_cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    Ok(app_dir.join("featured-marketplace.yaml"))
}

#[derive(Debug, serde::Deserialize)]
struct FeaturedRepositoriesWrapper {
    #[serde(default, alias = "featured_repositories")]
    repositories: Option<FeaturedRepositoriesConfig>,
}

fn parse_featured_repositories_yaml(
    yaml_content: &str,
) -> Result<FeaturedRepositoriesConfig, String> {
    let wrapper: FeaturedRepositoriesWrapper = serde_yaml::from_str(yaml_content)
        .map_err(|e| format!("Failed to parse featured repositories wrapper: {}", e))?;
    if let Some(config) = wrapper.repositories {
        return Ok(config);
    }

    serde_yaml::from_str::<FeaturedRepositoriesConfig>(yaml_content)
        .map_err(|e| format!("Failed to parse featured repositories: {}", e))
}

/// 获取精选仓库列表
#[tauri::command]
pub async fn get_featured_repositories(
    app: tauri::AppHandle,
) -> Result<FeaturedRepositoriesConfig, String> {
    // 1) 优先读取 app_data_dir 下的缓存文件（支持在线刷新后持久化）
    let cache_path = featured_repositories_cache_path(&app)?;
    if let Ok(cached_yaml) = std::fs::read_to_string(&cache_path) {
        match parse_featured_repositories_yaml(&cached_yaml) {
            Ok(config) => return Ok(config),
            Err(e) => {
                log::warn!(
                    "精选仓库缓存文件解析失败，将回退到内置默认配置: {:?}, 错误: {}",
                    cache_path,
                    e
                );
            }
        }
    }

    // 2) 回退到编译期内置的默认 YAML（用于首次启动/离线/打包环境）
    parse_featured_repositories_yaml(DEFAULT_FEATURED_REPOSITORIES_YAML)
}

/// 刷新精选仓库列表（从 GitHub 下载最新 YAML 并写入 app_data_dir 缓存）
#[tauri::command]
pub async fn refresh_featured_repositories(
    app: tauri::AppHandle,
) -> Result<FeaturedRepositoriesConfig, String> {
    use std::io::Write;

    let yaml_content = reqwest::Client::new()
        .get(FEATURED_REPOSITORIES_REMOTE_URL)
        .header(reqwest::header::USER_AGENT, "agent-skills-guard")
        .send()
        .await
        .map_err(|e| format!("Failed to download featured repositories: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Failed to download featured repositories: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read featured repositories content: {}", e))?;

    // 先校验解析成功，再落盘
    let config = parse_featured_repositories_yaml(&yaml_content)
        .map_err(|e| format!("Failed to parse downloaded featured repositories: {}", e))?;

    let cache_path = featured_repositories_cache_path(&app)?;
    let cache_dir = cache_path
        .parent()
        .ok_or_else(|| "Failed to get featured repositories cache directory".to_string())?;

    let mut tmp = tempfile::NamedTempFile::new_in(cache_dir)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(yaml_content.as_bytes())
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    if cache_path.exists() {
        let _ = std::fs::remove_file(&cache_path);
    }
    tmp.persist(&cache_path)
        .map_err(|e| format!("Failed to persist featured repositories cache: {}", e))?;

    Ok(config)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportFeaturedRepositoriesResult {
    pub total_count: usize,
    pub added_count: usize,
    pub skipped_count: usize,
}

/// 导入精选仓库到「我的仓库」
///
/// - 默认导入 `official` + `community`
/// - 会跳过已存在的仓库 URL，避免覆盖已有记录/扫描状态
#[tauri::command]
pub async fn import_featured_repositories(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    category_ids: Option<Vec<String>>,
) -> Result<ImportFeaturedRepositoriesResult, String> {
    let category_ids =
        category_ids.unwrap_or_else(|| vec!["official".to_string(), "community".to_string()]);

    let config = get_featured_repositories(app).await?;
    let mut existing_urls: std::collections::HashSet<String> = state
        .db
        .get_repositories()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| r.url)
        .collect();

    let mut total_count = 0usize;
    let mut added_count = 0usize;
    let mut skipped_count = 0usize;

    for category in config
        .categories
        .into_iter()
        .filter(|c| category_ids.contains(&c.id))
    {
        for repo in category.repositories {
            total_count += 1;

            if existing_urls.contains(&repo.url) {
                skipped_count += 1;
                continue;
            }

            let new_repo = Repository::new(repo.url.clone(), repo.name);
            state
                .db
                .add_repository(&new_repo)
                .map_err(|e| e.to_string())?;
            existing_urls.insert(repo.url);
            added_count += 1;
        }
    }

    Ok(ImportFeaturedRepositoriesResult {
        total_count,
        added_count,
        skipped_count,
    })
}

/// 重置应用的本地数据（数据库 + 缓存 + 配置文件）
///
/// 注意：不会删除用户自定义的技能安装目录中的文件，只会清空应用自身的索引/缓存。
#[tauri::command]
pub async fn reset_app_data(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // 1) 清空数据库内容（保留表结构与迁移）
    state
        .db
        .reset_all_data()
        .map_err(|e| format!("Failed to reset database: {}", e))?;

    // 2) 清理 app_data_dir 下除数据库文件外的内容（如精选仓库缓存、可能的设置文件）
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    if let Ok(entries) = std::fs::read_dir(&app_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

            // 数据库相关文件保持在位（连接仍在使用中；内容已在上方清空）
            if file_name.starts_with("agent-skills.db") {
                continue;
            }

            if path.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    log::warn!("清理 app data 子目录失败: {:?}, 错误: {}", path, e);
                }
            } else if path.exists() {
                if let Err(e) = std::fs::remove_file(&path) {
                    log::warn!("清理 app data 文件失败: {:?}, 错误: {}", path, e);
                }
            }
        }
    }

    // 3) 清理 cache_dir 下的应用缓存（仓库压缩包、staging、备份等）
    if let Some(cache_dir) = dirs::cache_dir() {
        let cache_root = cache_dir.join("agent-skills-guard");
        if cache_root.exists() {
            if let Err(e) = std::fs::remove_dir_all(&cache_root) {
                log::warn!("清理 cache_dir 失败: {:?}, 错误: {}", cache_root, e);
            }
        }
    }

    Ok(())
}

/// 检查仓库是否已添加
#[tauri::command]
pub async fn is_repository_added(state: State<'_, AppState>, url: String) -> Result<bool, String> {
    let repos = state.db.get_repositories().map_err(|e| e.to_string())?;

    Ok(repos.iter().any(|r| r.url == url))
}

/// 检查已安装技能的更新
/// 返回：Vec<(skill_id, latest_commit_sha)>
#[tauri::command]
pub async fn check_skills_updates(
    state: State<'_, AppState>,
) -> Result<Vec<(String, String)>, String> {
    let manager = state.skill_manager.lock().await;
    let installed_skills = manager.get_installed_skills().map_err(|e| e.to_string())?;

    let mut updates = Vec::new();

    for skill in installed_skills {
        // 跳过本地技能
        if skill.repository_url == "local" {
            continue;
        }

        // 解析仓库 URL
        let (owner, repo) = match Repository::from_github_url(&skill.repository_url) {
            Ok(result) => result,
            Err(e) => {
                log::warn!("无法解析仓库 URL {}: {}", skill.repository_url, e);
                continue;
            }
        };

        // 检查更新
        match state
            .github
            .check_skill_update(
                &owner,
                &repo,
                &skill.file_path,
                skill.installed_commit_sha.as_deref(),
            )
            .await
        {
            Ok(Some(latest_sha)) => {
                log::info!("技能 {} 有更新可用: {}", skill.name, latest_sha);
                updates.push((skill.id.clone(), latest_sha));
            }
            Ok(None) => {
                log::debug!("技能 {} 无更新", skill.name);
            }
            Err(e) => {
                log::warn!("检查技能 {} 更新时出错: {}", skill.name, e);
            }
        }
    }

    log::info!("检查更新完成，发现 {} 个技能有更新", updates.len());
    Ok(updates)
}

/// 准备技能更新
#[tauri::command]
pub async fn prepare_skill_update(
    state: State<'_, AppState>,
    skill_id: String,
    locale: String,
) -> Result<(crate::models::security::SecurityReport, Vec<String>), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .prepare_skill_update(&skill_id, &locale)
        .await
        .map_err(|e| e.to_string())
}

/// 确认技能更新
#[tauri::command]
pub async fn confirm_skill_update(
    state: State<'_, AppState>,
    skill_id: String,
    force_overwrite: bool,
    allow_partial_scan: Option<bool>,
) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .confirm_skill_update(
            &skill_id,
            force_overwrite,
            allow_partial_scan.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// 取消技能更新
#[tauri::command]
pub async fn cancel_skill_update(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<(), String> {
    let manager = state.skill_manager.lock().await;
    manager
        .cancel_skill_update(&skill_id)
        .map_err(|e| e.to_string())
}

/// 检查并自动扫描未扫描的仓库（用于首次启动）
#[tauri::command]
pub async fn auto_scan_unscanned_repositories(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    // 获取所有未扫描的仓库
    let unscanned_repos = state
        .db
        .get_unscanned_repositories()
        .map_err(|e| e.to_string())?;

    if unscanned_repos.is_empty() {
        log::info!("没有需要自动扫描的仓库");
        return Ok(vec![]);
    }

    log::info!(
        "发现 {} 个未扫描的仓库，开始自动扫描...",
        unscanned_repos.len()
    );

    let mut scanned_repos = Vec::new();

    // 逐个扫描仓库
    for repo_id in unscanned_repos {
        log::info!("自动扫描仓库: {}", repo_id);

        match scan_repository(state.clone(), repo_id.clone()).await {
            Ok(skills) => {
                log::info!("仓库 {} 扫描成功，发现 {} 个技能", repo_id, skills.len());
                scanned_repos.push(repo_id);
            }
            Err(e) => {
                log::warn!("仓库 {} 扫描失败: {}", repo_id, e);
                // 继续扫描下一个仓库，不中断整个流程
            }
        }
    }

    log::info!("自动扫描完成，成功扫描 {} 个仓库", scanned_repos.len());
    Ok(scanned_repos)
}
