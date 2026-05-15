use crate::models::{Skill, LOCAL_REPOSITORY_URL};
use crate::security::{ScanOptions, SecurityScanner};
use crate::services::agent_tools::AgentTool;
use crate::services::link_fs;
use crate::services::{Database, GitHubService};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SkillManager {
    db: Arc<Database>,
    github: GitHubService,
    scanner: SecurityScanner,
    skills_dir: PathBuf,
    cleanup_done: AtomicBool,
    installing: std::sync::Mutex<std::collections::HashSet<String>>,
    installed_cache: std::sync::Mutex<Option<(std::time::Instant, Vec<Skill>)>>,
}

fn build_synced_tool_state(
    source: &Path,
    skill_dir_name: &str,
    target_tools: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut linked_tool_ids: Vec<String> = Vec::new();
    let mut paths: Vec<String> = vec![source.to_string_lossy().to_string()];

    for tool_id in target_tools {
        let Some(tool) = AgentTool::from_id(tool_id) else {
            continue;
        };
        if tool == AgentTool::Agents || linked_tool_ids.iter().any(|id| id == tool.id()) {
            continue;
        }

        let Some(tool_dir) = tool.default_skills_dir() else {
            continue;
        };

        linked_tool_ids.push(tool.id().to_string());
        paths.push(tool_dir.join(skill_dir_name).to_string_lossy().to_string());
    }

    (linked_tool_ids, paths)
}

fn build_local_skill_id(checksum: &str, local_path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let mut normalized_path = local_path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized_path = normalized_path.to_ascii_lowercase();
    }
    let mut hasher = Sha256::new();
    hasher.update(checksum.as_bytes());
    hasher.update(b":");
    hasher.update(normalized_path.as_bytes());
    let digest = hex::encode(hasher.finalize());

    format!("local::{}", &digest[..16])
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

fn paths_point_to_same_location(left: &Path, right: &Path) -> bool {
    if normalize_path_for_compare(left) == normalize_path_for_compare(right) {
        return true;
    }

    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => {
            normalize_path_for_compare(&left) == normalize_path_for_compare(&right)
        }
        _ => false,
    }
}

fn skill_md_checksum(skill_dir: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};

    let content = std::fs::read(skill_dir.join("SKILL.md")).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(content);
    Some(hex::encode(hasher.finalize()))
}

fn tool_skill_path_is_compatible_with_source(
    source: &Path,
    tool_skill_path: &Path,
    source_checksum: Option<&str>,
) -> bool {
    if paths_point_to_same_location(source, tool_skill_path) {
        return true;
    }

    let Some(tool_checksum) = skill_md_checksum(tool_skill_path) else {
        return false;
    };
    // A third-party install may be a real directory rather than a link. Treat it as the
    // same skill only when SKILL.md matches; this is compatibility detection, not trust.
    let expected_checksum =
        skill_md_checksum(source).or_else(|| source_checksum.map(str::to_owned));

    expected_checksum.as_deref() == Some(tool_checksum.as_str())
}

fn path_is_inside_dir_resolving_links(path: &Path, dir: &Path) -> bool {
    if path.starts_with(dir) {
        return true;
    }

    match (std::fs::canonicalize(path), std::fs::canonicalize(dir)) {
        (Ok(path), Ok(dir)) => path.starts_with(dir),
        _ => false,
    }
}

fn find_tool_id_for_scan_dir(
    scan_dir: &Path,
    dir_to_tool: &HashMap<PathBuf, String>,
) -> Option<String> {
    if let Some(tool_id) = dir_to_tool.get(scan_dir) {
        return Some(tool_id.clone());
    }

    dir_to_tool
        .iter()
        .find(|(dir, _)| paths_point_to_same_location(scan_dir, dir))
        .map(|(_, tool_id)| tool_id.clone())
}

fn push_unique_value(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn default_tool_dirs() -> Vec<(PathBuf, String)> {
    AgentTool::all()
        .into_iter()
        .filter_map(|tool| {
            tool.default_skills_dir()
                .map(|dir| (dir, tool.id().to_string()))
        })
        .collect()
}

fn skill_candidate_paths(skill: &Skill) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(path) = &skill.source_path {
        push_unique_value(&mut paths, path.clone());
    }
    if let Some(path) = &skill.local_path {
        push_unique_value(&mut paths, path.clone());
    }
    if let Some(local_paths) = &skill.local_paths {
        for path in local_paths {
            push_unique_value(&mut paths, path.clone());
        }
    }
    paths
}

fn refresh_existing_tool_links_for_skill(skill: &Skill, tool_dirs: &[(PathBuf, String)]) -> Skill {
    if !skill.installed {
        return skill.clone();
    }

    let candidate_paths = skill_candidate_paths(skill);
    let Some(source_path) = candidate_paths
        .iter()
        .find(|path| PathBuf::from(path).exists())
        .or_else(|| candidate_paths.first())
    else {
        return skill.clone();
    };

    let source = PathBuf::from(source_path);
    let Some(skill_dir_name) = source.file_name() else {
        return skill.clone();
    };

    let mut refreshed = skill.clone();
    let mut local_paths = Vec::new();
    for candidate_path in &candidate_paths {
        let candidate = PathBuf::from(candidate_path);
        if candidate.exists()
            && tool_skill_path_is_compatible_with_source(
                &source,
                &candidate,
                skill.checksum.as_deref(),
            )
        {
            push_unique_value(&mut local_paths, candidate_path.clone());
        }
    }

    let mut linked_tools = Vec::new();

    for (tool_dir, tool_id) in tool_dirs {
        let tool_skill_path = tool_dir.join(skill_dir_name);
        if !tool_skill_path.exists() {
            continue;
        }
        if !tool_skill_path_is_compatible_with_source(
            &source,
            &tool_skill_path,
            skill.checksum.as_deref(),
        ) {
            continue;
        }

        push_unique_value(
            &mut local_paths,
            tool_skill_path.to_string_lossy().to_string(),
        );
        if tool_id != AgentTool::Agents.id() {
            push_unique_value(&mut linked_tools, tool_id.clone());
        }
    }

    if local_paths.is_empty() {
        return skill.clone();
    }

    let current_source_exists = skill
        .source_path
        .as_deref()
        .map(|path| PathBuf::from(path).exists())
        .unwrap_or(false);
    if !current_source_exists {
        // Older builds could persist ~/.agents as the source even after that directory was
        // removed. Move the canonical source to a compatible existing tool path so the UI
        // no longer lights up a missing .agents location.
        refreshed.source_path = local_paths.first().cloned();
    }
    refreshed.local_path = local_paths.first().cloned();
    refreshed.local_paths = Some(local_paths);
    refreshed.linked_tools = linked_tools;
    refreshed
}

fn installed_tool_state_changed(previous: &Skill, refreshed: &Skill) -> bool {
    previous.source_path != refreshed.source_path
        || previous.local_path != refreshed.local_path
        || previous.local_paths != refreshed.local_paths
        || previous.linked_tools != refreshed.linked_tools
}

fn restore_installation_backup(backup_path: &Path, final_install_dir: &Path) -> Result<()> {
    if final_install_dir.exists() {
        let metadata = std::fs::symlink_metadata(final_install_dir)
            .with_context(|| format!("无法读取残留安装目录: {:?}", final_install_dir))?;
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            std::fs::remove_dir_all(final_install_dir)
                .with_context(|| format!("无法清理残留安装目录: {:?}", final_install_dir))?;
        } else {
            std::fs::remove_file(final_install_dir)
                .with_context(|| format!("无法清理残留安装路径: {:?}", final_install_dir))?;
        }
    }

    rename_with_retry(backup_path, final_install_dir).with_context(|| {
        format!(
            "无法还原安装备份: {:?} -> {:?}",
            backup_path, final_install_dir
        )
    })
}

fn resolve_update_target_install_dir(raw: &Path) -> PathBuf {
    if !link_fs::is_dir_link(raw) {
        return raw.to_path_buf();
    }

    let linked_target = match link_fs::read_dir_link_target(raw) {
        Ok(target) => target,
        Err(_) => return std::fs::canonicalize(raw).unwrap_or_else(|_| raw.to_path_buf()),
    };

    let resolved = if linked_target.is_absolute() {
        linked_target
    } else {
        raw.parent()
            .map(|parent| parent.join(&linked_target))
            .unwrap_or(linked_target)
    };

    std::fs::canonicalize(&resolved).unwrap_or(resolved)
}

fn resolve_update_install_paths(raw: &Path) -> (PathBuf, PathBuf) {
    (raw.to_path_buf(), resolve_update_target_install_dir(raw))
}

impl SkillManager {
    pub fn new(db: Arc<Database>) -> Self {
        let skills_dir = Self::get_skills_directory();

        Self {
            db,
            github: GitHubService::new(),
            scanner: SecurityScanner::new(),
            skills_dir,
            cleanup_done: AtomicBool::new(false),
            installing: std::sync::Mutex::new(std::collections::HashSet::new()),
            installed_cache: std::sync::Mutex::new(None),
        }
    }

    /// 获取统一源 skills 安装目录（~/.agents/skills）
    fn get_skills_directory() -> PathBuf {
        AgentTool::Agents.default_skills_dir().unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".agents").join("skills")
        })
    }

    fn create_temp_install_dir(
        &self,
        install_base_dir: &Path,
        skill_folder_name: &str,
    ) -> Result<PathBuf> {
        let temp_dir = install_base_dir.join(format!(
            ".{}.tmp-{}",
            skill_folder_name,
            uuid::Uuid::new_v4()
        ));
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir)
                .context("无法清理旧的临时安装目录，请检查文件权限")?;
        }
        std::fs::create_dir_all(&temp_dir).context("无法创建临时安装目录，请检查磁盘权限")?;
        Ok(temp_dir)
    }

    fn replace_installation_directory(
        &self,
        prepared_dir: &Path,
        final_install_dir: &Path,
    ) -> Result<()> {
        let mut backup_dir = None;

        if final_install_dir.exists() {
            let file_name = final_install_dir
                .file_name()
                .and_then(|name| name.to_str())
                .context("无效的技能目录名")?;
            let parent = final_install_dir.parent().context("无效的安装目录")?;
            let backup_path =
                parent.join(format!(".{}.backup-{}", file_name, uuid::Uuid::new_v4()));

            rename_with_retry(final_install_dir, &backup_path).with_context(|| {
                format!(
                    "无法备份现有技能目录，请关闭占用该目录的程序: {:?}",
                    final_install_dir
                )
            })?;
            backup_dir = Some(backup_path);
        }

        match rename_with_retry(prepared_dir, final_install_dir) {
            Ok(()) => {
                if let Some(backup_path) = backup_dir {
                    if let Err(error) = std::fs::remove_dir_all(&backup_path) {
                        log::warn!("清理安装备份目录失败: {:?}, 错误: {}", backup_path, error);
                    }
                }
                Ok(())
            }
            Err(error) => {
                let mut restore_error = None;
                if let Some(backup_path) = backup_dir {
                    // 先尝试将备份还原到原位，再清理临时目录
                    if let Err(restore_err) =
                        restore_installation_backup(&backup_path, final_install_dir)
                    {
                        log::error!(
                            "还原安装备份失败: {:?} -> {:?}, 错误: {}",
                            backup_path,
                            final_install_dir,
                            restore_err
                        );
                        restore_error = Some(restore_err);
                    }
                }
                if prepared_dir.exists() {
                    let _ = std::fs::remove_dir_all(prepared_dir);
                }
                if let Some(restore_error) = restore_error {
                    Err(anyhow::anyhow!(
                        "无法替换技能目录: {:?} -> {:?}, 错误: {}；同时无法还原原安装目录: {}",
                        prepared_dir,
                        final_install_dir,
                        error,
                        restore_error
                    ))
                } else {
                    Err(anyhow::anyhow!(
                        "无法替换技能目录: {:?} -> {:?}, 错误: {}",
                        prepared_dir,
                        final_install_dir,
                        error
                    ))
                }
            }
        }
    }

    fn apply_scan_report(skill: &mut Skill, report: &crate::models::SecurityReport) {
        skill.security_score = Some(report.score);
        skill.security_level = Some(report.level.as_str().to_string());
        skill.security_issues = Some(report.issues.clone());
        skill.security_report = Some(report.clone());
        skill.scanned_at = Some(Utc::now());
    }

    fn enforce_installable_report(
        &self,
        report: &crate::models::SecurityReport,
        operation: &str,
        allow_partial_scan: bool,
    ) -> Result<()> {
        if report.blocked || !report.hard_trigger_issues.is_empty() {
            let mut error_msg = format!(
                "SECURITY_CHECK_BLOCKED: {}\n",
                operation
            );
            for (idx, issue) in report.hard_trigger_issues.iter().enumerate() {
                error_msg.push_str(&format!("{}. {}\n", idx + 1, issue));
            }
            anyhow::bail!(error_msg);
        }

        if report.partial_scan && !allow_partial_scan {
            let mut error_msg = format!(
                "SECURITY_PARTIAL_SCAN_BLOCKED: {}\n",
                operation
            );
            if report.skipped_files.is_empty() {
                error_msg.push_str("1. 扫描过程中存在被截断或跳过的文件\n");
            } else {
                for (idx, file) in report.skipped_files.iter().enumerate() {
                    error_msg.push_str(&format!("{}. {}\n", idx + 1, file));
                }
            }
            anyhow::bail!(error_msg);
        }

        Ok(())
    }

    fn rescan_skill_directory_for_confirmation(
        &self,
        dir: &Path,
        skill_id: &str,
        allow_partial_scan: bool,
    ) -> Result<crate::models::SecurityReport> {
        let locale = rust_i18n::locale();
        let report = self.scanner.scan_directory_with_options(
            dir.to_str().context("技能目录路径无效")?,
            skill_id,
            &locale,
            ScanOptions { skip_readme: true },
            None,
        )?;
        self.enforce_installable_report(&report, "安装或更新技能", allow_partial_scan)?;
        Ok(report)
    }

    /// 下载并分析 skill，返回文件内容和安全报告
    pub async fn download_and_analyze(
        &self,
        skill: &mut Skill,
    ) -> Result<(Vec<u8>, crate::models::SecurityReport)> {
        // 构建下载 URL
        let (owner, repo) = crate::models::Repository::from_github_url(&skill.repository_url)?;

        // 尝试多个分支下载 SKILL.md 文件
        let mut content = None;
        let mut last_error = None;

        for branch in super::github::DEFAULT_BRANCHES {
            let download_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md",
                owner, repo, branch, skill.file_path
            );

            log::info!("尝试从分支 {} 下载 SKILL.md: {}", branch, download_url);

            match self.github.download_file(&download_url).await {
                Ok(file_content) => {
                    log::info!("成功从分支 {} 下载 SKILL.md", branch);
                    content = Some(file_content);
                    break;
                }
                Err(e) => {
                    log::info!("分支 {} 下载失败: {}", branch, e);
                    last_error = Some(e);
                    continue;
                }
            }
        }

        let content = content.ok_or_else(|| {
            last_error.unwrap_or_else(|| anyhow::anyhow!("所有分支均无法下载 SKILL.md"))
        })?;

        // 解析 frontmatter 更新 skill 元数据
        let (name, description) = self
            .github
            .fetch_skill_metadata(&owner, &repo, &skill.file_path)
            .await?;
        skill.name = name;
        skill.description = description;

        // 安全扫描
        let content_str = String::from_utf8_lossy(&content);
        let locale = rust_i18n::locale();
        let report = self.scanner.scan_file(&content_str, "SKILL.md", &locale)?;

        // 更新 skill 信息
        Self::apply_scan_report(skill, &report);
        skill.checksum = Some(self.scanner.calculate_checksum(&content));

        Ok((content, report))
    }

    /// 安装 skill 到本地（旧入口，内部委托给 prepare + confirm）
    pub async fn install_skill(
        &self,
        skill_id: &str,
        install_path: Option<String>,
        allow_partial_scan: bool,
    ) -> Result<()> {
        let locale = rust_i18n::locale();
        self.prepare_skill_installation(skill_id, &locale).await?;
        self.confirm_skill_installation(skill_id, install_path, allow_partial_scan, Vec::new())?;
        Ok(())
    }

    /// 准备安装技能：扫描缓存中的技能，但不复制文件，不标记为已安装
    /// 返回扫描报告供前端判断是否需要用户确认
    pub async fn prepare_skill_installation(
        &self,
        skill_id: &str,
        locale: &str,
    ) -> Result<crate::models::security::SecurityReport> {
        use anyhow::Context;

        log::info!("Preparing installation for skill: {}", skill_id);

        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;
        let (_skill_md_content, _report) = self.download_and_analyze(&mut skill).await?;

        // 获取仓库记录
        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("未找到对应的仓库记录")?
            .clone();

        // 确保仓库缓存存在
        let cache_path = if let Some(existing_cache_path) = &repo.cache_path {
            // 验证缓存路径是否存在
            let cache_path_buf = PathBuf::from(existing_cache_path);
            if cache_path_buf.exists() {
                existing_cache_path.clone()
            } else {
                // 缓存路径不存在，重新下载
                log::warn!("缓存路径不存在，重新下载仓库: {:?}", cache_path_buf);
                self.download_and_cache_repository(&repo.id, &skill.repository_url)
                    .await?
            }
        } else {
            // 仓库缓存不存在，自动下载
            log::info!("仓库缓存不存在，自动下载: {}", skill.repository_url);
            self.download_and_cache_repository(&repo.id, &skill.repository_url)
                .await?
        };

        // 定位缓存中的技能目录
        log::info!("从仓库缓存定位技能: {:?}", cache_path);
        let skill_cache_dir =
            self.locate_skill_in_cache(PathBuf::from(&cache_path).as_path(), &skill.file_path)?;

        log::info!("在缓存中找到技能目录: {:?}", skill_cache_dir);

        // 直接扫描缓存中的技能目录
        let scan_report = self.scanner.scan_directory_with_options(
            skill_cache_dir.to_str().context("技能目录路径无效")?,
            &skill.id,
            locale,
            ScanOptions { skip_readme: true },
            None,
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        // 更新 skill 安全信息到数据库（但不标记为已安装）
        Self::apply_scan_report(&mut skill, &scan_report);
        // 使用 __cache__: 前缀标记临时缓存路径，避免 scan_local_skills 误认为已安装
        skill.local_path = Some(format!("__cache__:{}", skill_cache_dir.to_string_lossy()));

        // 保存安全信息到数据库，但不标记为已安装
        self.db.save_skill(&skill)?;

        self.enforce_installable_report(&scan_report, "准备安装技能", false)?;

        log::info!("Skill prepared successfully, scanned from cache, awaiting user confirmation");
        Ok(scan_report)
    }

    /// 下载并缓存仓库
    async fn download_and_cache_repository(&self, repo_id: &str, repo_url: &str) -> Result<String> {
        use anyhow::Context;

        log::info!("Downloading and caching repository: {}", repo_url);

        // 解析 GitHub URL
        let (owner, repo_name) = crate::models::Repository::from_github_url(repo_url)?;

        // 获取缓存基础目录
        let cache_base_dir = dirs::cache_dir()
            .context("无法获取系统缓存目录")?
            .join("agent-skills-guard")
            .join("repositories");

        // 下载仓库压缩包并解压
        let (extract_dir, commit_sha) = self
            .github
            .download_repository_archive(&owner, &repo_name, &cache_base_dir)
            .await
            .context("下载仓库压缩包失败")?;

        let cache_path_str = extract_dir.to_string_lossy().to_string();

        // 更新数据库缓存信息
        self.db
            .update_repository_cache(repo_id, &cache_path_str, Utc::now(), Some(&commit_sha))
            .context("更新仓库缓存信息失败")?;

        log::info!("Repository cached successfully: {}", cache_path_str);

        Ok(cache_path_str)
    }

    /// 在仓库缓存中定位技能目录
    fn locate_skill_in_cache(
        &self,
        cache_path: &std::path::Path,
        skill_file_path: &str,
    ) -> Result<PathBuf> {
        // 找到仓库根目录（cache_path 指向 extracted/ 目录）
        let repo_root = self.find_repo_root_in_cache(cache_path)?;

        // 构建技能在缓存中的路径
        let skill_cache_path = if skill_file_path == "." {
            repo_root.clone()
        } else {
            repo_root.join(skill_file_path)
        };

        if !skill_cache_path.exists() {
            anyhow::bail!("缓存中未找到技能目录: {:?}", skill_cache_path);
        }

        Ok(skill_cache_path)
    }

    /// 找到GitHub zipball解压后的根目录
    fn find_repo_root_in_cache(&self, extract_dir: &std::path::Path) -> Result<PathBuf> {
        use anyhow::Context;

        // GitHub zipball解压后会有一个 {owner}-{repo}-{commit}/ 目录
        for entry in std::fs::read_dir(extract_dir).context("无法读取解压目录")? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // 第一个目录就是仓库根目录
                return Ok(path);
            }
        }

        anyhow::bail!("REPOSITORY_ROOT_NOT_FOUND")
    }

    /// 递归复制目录
    /// counter: 文件计数器，传入 &mut 变量可统计复制的文件数
    fn copy_dir_recursive(
        &self,
        src: &std::path::Path,
        dst: &std::path::Path,
        counter: &mut usize,
    ) -> Result<()> {
        use anyhow::Context;

        if !dst.exists() {
            std::fs::create_dir_all(dst).context(format!("无法创建目标目录: {:?}", dst))?;
        }

        for entry in std::fs::read_dir(src).context(format!("无法读取源目录: {:?}", src))? {
            let entry = entry.context(format!("读取目录项失败: {:?}", src))?;
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);
            let file_type = entry
                .file_type()
                .context(format!("无法获取文件类型: {:?}", src_path))?;

            if file_type.is_symlink() {
                log::warn!("跳过符号链接（源目录中不允许）: {:?}", src_path);
                continue;
            } else if file_type.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path, counter)?;
            } else if file_type.is_file() {
                // 确保目标文件的父目录存在
                if let Some(parent) = dst_path.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent)
                            .context(format!("无法创建文件父目录: {:?}", parent))?;
                    }
                }

                match std::fs::copy(&src_path, &dst_path) {
                    Ok(bytes) => {
                        *counter += 1;
                        log::debug!("已复制文件: {:?} ({} bytes)", file_name, bytes);
                    }
                    Err(e) => {
                        let error_msg = if e.raw_os_error() == Some(5) {
                            format!(
                                "复制文件失败（拒绝访问）\n文件: {:?}\n\n可能原因：\n1. 目标文件正在被其他程序使用\n2. 文件被设置为只读\n3. 权限不足\n4. 杀毒软件拦截\n\n建议：\n1. 关闭可能打开该文件的程序\n2. 检查文件是否为只读\n3. 以管理员权限运行\n\n原始错误: {}",
                                file_name, e
                            )
                        } else {
                            format!(
                                "复制文件失败\n源: {:?}\n目标: {:?}\n错误: {}",
                                src_path, dst_path, e
                            )
                        };
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }
            }
        }

        Ok(())
    }

    /// 确认安装技能：从缓存复制到目标路径，标记为已安装
    /// target_tools: 要同步链接的工具 id 列表（除 "agents" 外的工具）
    pub fn confirm_skill_installation(
        &self,
        skill_id: &str,
        install_path: Option<String>,
        allow_partial_scan: bool,
        target_tools: Vec<String>,
    ) -> Result<()> {
        log::info!("Confirming installation for skill: {}", skill_id);

        {
            let mut installing = self.installing.lock().unwrap();
            if installing.contains(skill_id) {
                anyhow::bail!("SKILL_INSTALL_IN_PROGRESS");
            }
            installing.insert(skill_id.to_string());
        }

        let result = self.confirm_skill_installation_inner(
            skill_id,
            install_path,
            allow_partial_scan,
            target_tools,
        );
        self.installing.lock().unwrap().remove(skill_id);
        result
    }

    /// Inner implementation of confirm_skill_installation (called after installing guard is set)
    fn confirm_skill_installation_inner(
        &self,
        skill_id: &str,
        install_path: Option<String>,
        allow_partial_scan: bool,
        target_tools: Vec<String>,
    ) -> Result<()> {
        use anyhow::Context;
        use std::path::PathBuf;

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 获取缓存中的技能路径（prepare阶段保存的，可能带 __cache__: 前缀）
        let raw_cache_path = skill
            .local_path
            .as_ref()
            .context("技能尚未准备，请先调用prepare_skill_installation")?;
        let cache_path = raw_cache_path
            .strip_prefix("__cache__:")
            .unwrap_or(raw_cache_path);
        let cache_dir = PathBuf::from(cache_path);

        // 获取仓库的 cached_commit_sha
        let repositories = self.db.get_repositories()?;
        let repo = repositories.iter().find(|r| r.url == skill.repository_url);
        let commit_sha = repo.and_then(|r| r.cached_commit_sha.clone());

        // 确定最终安装路径
        let install_base_dir = if let Some(user_path) = install_path {
            PathBuf::from(user_path)
        } else {
            self.skills_dir.clone()
        };

        // 获取技能目录名
        let skill_dir_name = cache_dir.file_name().context("无效的技能目录名")?;
        let final_install_dir = install_base_dir.join(skill_dir_name);

        // 确保目标基础目录存在
        std::fs::create_dir_all(&install_base_dir).context("无法创建目标目录")?;

        let skill_dir_name = skill_dir_name.to_string_lossy().to_string();
        let prepared_dir = self.create_temp_install_dir(&install_base_dir, &skill_dir_name)?;

        let scan_report = self.rescan_skill_directory_for_confirmation(
            &cache_dir,
            &skill.id,
            allow_partial_scan,
        )?;

        // 从缓存复制到目标路径
        log::info!(
            "Copying skill from cache {:?} to {:?}",
            cache_dir,
            prepared_dir
        );
        let mut files_copied = 0;
        self.copy_dir_recursive(&cache_dir, &prepared_dir, &mut files_copied)?;

        log::info!(
            "Copied {} files from cache to install directory",
            files_copied
        );

        self.replace_installation_directory(&prepared_dir, &final_install_dir)?;

        // 更新源路径（~/.agents/skills/<name>）
        let source_path_str = final_install_dir.to_string_lossy().to_string();

        // 为其他工具创建 Junction/symlink 链接
        let mut linked_tool_ids: Vec<String> = Vec::new();
        let non_agent_tools: Vec<AgentTool> = target_tools
            .iter()
            .filter_map(|id| AgentTool::from_id(id))
            .filter(|t| t != &AgentTool::Agents)
            .collect();

        for tool in &non_agent_tools {
            if let Some(tool_dir) = tool.default_skills_dir() {
                let link_path = tool_dir.join(skill_dir_name.as_str());

                // 兼容性检查：如果目标已存在且内容不同，不覆盖（与 sync_skill_to_tools 一致）
                if link_path.exists() || link_fs::is_dir_link(&link_path) {
                    if tool_skill_path_is_compatible_with_source(
                        &final_install_dir,
                        &link_path,
                        None,
                    ) {
                        log::info!("复用已存在的兼容工具路径 [{:?}]", link_path);
                        linked_tool_ids.push(tool.id().to_string());
                        continue;
                    } else {
                        log::warn!(
                            "工具 '{}' 下已存在同名但内容不同的技能，不覆盖: {:?}",
                            tool.id(),
                            link_path
                        );
                        continue;
                    }
                }

                match link_fs::create_dir_link(&final_install_dir, &link_path) {
                    Ok(()) => {
                        log::info!("链接创建成功 [{:?}]: {:?}", tool.id(), link_path);
                        // Don't add to linked_tool_ids if source and link are the same location
                        if !paths_point_to_same_location(&final_install_dir, &link_path) {
                            linked_tool_ids.push(tool.id().to_string());
                        }
                    }
                    Err(e) => {
                        log::warn!("链接创建失败 [{:?}]: {}", tool.id(), e);
                    }
                }
            }
        }

        // 更新 local_path（向后兼容）
        skill.local_path = Some(source_path_str.clone());

        // 如果用户选择了非 agents 工具但全部失败，保存源路径后返回错误让前端提示
        // 此时源目录已创建，DB 记录已更新，用户可通过 sync_skill_to_tools 重试链接
        if !non_agent_tools.is_empty() && linked_tool_ids.is_empty() {
            skill.source_path = Some(source_path_str.clone());
            skill.local_paths = Some(vec![source_path_str.clone()]);
            skill.linked_tools = Vec::new();
            skill.is_local_only = false;
            skill.installed = true;
            skill.installed_at = Some(Utc::now());
            skill.installed_commit_sha = commit_sha;
            Self::apply_scan_report(&mut skill, &scan_report);
            self.db.save_skill(&skill)?;
            anyhow::bail!(
                "LINK_CREATION_ALL_FAILED: {:?}",
                non_agent_tools.iter().map(|t| t.id()).collect::<Vec<_>>()
            );
        }
        skill.local_path = Some(source_path_str.clone());

        // local_paths 包含源 + 所有成功创建的链接路径（与 linked_tool_ids 保持一致）
        let (linked_tool_ids, paths) = build_synced_tool_state(
            &final_install_dir,
            skill_dir_name.as_str(),
            &linked_tool_ids,
        );
        skill.local_paths = Some(paths);

        // 设置新字段
        skill.source_path = Some(source_path_str);
        skill.linked_tools = linked_tool_ids;
        skill.is_local_only = false;

        // 标记为已安装
        skill.installed = true;
        skill.installed_at = Some(Utc::now());
        skill.installed_commit_sha = commit_sha;
        Self::apply_scan_report(&mut skill, &scan_report);

        self.db.save_skill(&skill)?;

        log::info!("Skill installation confirmed: {}", skill.name);
        self.invalidate_installed_cache();
        Ok(())
    }

    /// 取消安装技能：清除准备阶段的数据（不删除缓存）
    pub fn cancel_skill_installation(&self, skill_id: &str) -> Result<()> {
        use anyhow::Context;

        log::info!("Canceling installation for skill: {}", skill_id);

        if self.installing.lock().unwrap().contains(skill_id) {
            anyhow::bail!("SKILL_INSTALL_IN_PROGRESS");
        }

        let skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 注意：不删除缓存中的文件，因为缓存是共享的仓库缓存
        // 只清除数据库中的准备阶段信息

        // 清除数据库中的安全信息和本地路径
        let mut skill = skill;
        skill.local_path = None;
        skill.security_score = None;
        skill.security_level = None;
        skill.security_issues = None;
        skill.security_report = None;
        skill.scanned_at = None;

        self.db.save_skill(&skill)?;

        log::info!("Skill installation canceled: {}", skill.name);
        self.invalidate_installed_cache();
        Ok(())
    }

    /// 清理 DB 中残留的 __cache__: / __staging__: 临时路径
    /// 在 prepare 后用户未 confirm/cancel 就关闭 App 时，这些路径会残留
    pub fn cleanup_stale_prepare_paths(&self) -> Result<usize> {
        let skills = self.db.get_skills()?;
        let mut cleaned = 0usize;

        for skill in &skills {
            let mut needs_update = false;
            let mut updated = skill.clone();

            // 检查 local_path 是否为残留临时路径
            if let Some(local_path) = &skill.local_path {
                let is_temp = local_path.starts_with("__cache__:")
                    || local_path.starts_with("__staging__:");
                if is_temp {
                    let actual_path = local_path
                        .strip_prefix("__cache__:")
                        .or_else(|| local_path.strip_prefix("__staging__:"))
                        .unwrap_or(local_path);

                    if !PathBuf::from(actual_path).exists() {
                        if !skill.installed {
                            updated.local_path = None;
                            updated.security_score = None;
                            updated.security_level = None;
                            updated.security_issues = None;
                            updated.security_report = None;
                            updated.scanned_at = None;
                        } else {
                            updated.local_path = skill.local_paths.as_ref()
                                .and_then(|paths| paths.last().cloned());
                        }
                        needs_update = true;
                        log::info!(
                            "清理残留临时路径 (local_path): skill={}, path={}",
                            skill.name,
                            local_path
                        );
                    }
                }
            }

            // 检查 local_paths 中是否包含残留临时路径
            if let Some(local_paths) = &skill.local_paths {
                let cleaned_paths: Vec<String> = local_paths
                    .iter()
                    .filter(|p| {
                        let is_temp = p.starts_with("__cache__:")
                            || p.starts_with("__staging__:");
                        if !is_temp {
                            return true; // 保留非临时路径
                        }
                        let actual = p
                            .strip_prefix("__cache__:")
                            .or_else(|| p.strip_prefix("__staging__:"))
                            .unwrap_or(p.as_str());
                        let keep = PathBuf::from(actual).exists();
                        if !keep {
                            log::info!(
                                "清理残留临时路径 (local_paths): skill={}, path={}",
                                skill.name,
                                p
                            );
                        }
                        keep
                    })
                    .cloned()
                    .collect();
                if cleaned_paths.len() < local_paths.len() {
                    updated.local_paths = Some(cleaned_paths);
                    needs_update = true;
                }
            }

            if needs_update {
                self.db.save_skill(&updated)?;
                cleaned += 1;
            }
        }

        // Second pass: delete orphaned __cache__ skills where cache dir no longer exists
        let orphaned: Vec<String> = skills.iter()
            .filter(|s| {
                if s.installed { return false; }
                if let Some(local_path) = &s.local_path {
                    if local_path.starts_with("__cache__:") || local_path.starts_with("__staging__:") {
                        let actual_path = local_path
                            .strip_prefix("__cache__:")
                            .or_else(|| local_path.strip_prefix("__staging__:"))
                            .unwrap_or(local_path);
                        return !PathBuf::from(actual_path).exists();
                    }
                }
                false
            })
            .map(|s| s.id.clone())
            .collect();

        for skill_id in orphaned {
            log::info!("清理孤立的缓存技能记录: {}", skill_id);
            let _ = self.db.delete_skill(&skill_id);
        }

        Ok(cleaned)
    }

    /// 卸载 skill
    pub fn uninstall_skill(&self, skill_id: &str) -> Result<()> {
        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        let mut errors: Vec<String> = Vec::new();

        // 分离链接和源：先删所有链接（避免源先删后链接变成悬空 Junction）
        // 第一遍：删除链接（Junction/symlink）
        if let Some(local_paths) = &skill.local_paths {
            for local_path in local_paths {
                let path = PathBuf::from(local_path);
                if link_fs::is_dir_link(&path) {
                    if let Err(e) = link_fs::remove_dir_link(&path) {
                        let msg = format!("删除技能链接失败: {:?}, 错误: {}", path, e);
                        log::warn!("{}", msg);
                        errors.push(msg);
                    }
                }
            }
        }

        // 第二遍：删除真实目录/文件（source 及降级拷贝）
        if let Some(local_paths) = &skill.local_paths {
            for local_path in local_paths {
                let path = PathBuf::from(local_path);
                if link_fs::is_dir_link(&path) {
                    continue; // 已在第一遍处理
                }
                if path.exists() {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            let msg = format!("删除技能目录失败: {:?}, 错误: {}", path, e);
                            log::warn!("{}", msg);
                            errors.push(msg);
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        let msg = format!("删除技能文件失败: {:?}, 错误: {}", path, e);
                        log::warn!("{}", msg);
                        errors.push(msg);
                    }
                }
            }
        }

        // 向后兼容:如果 local_paths 为空,尝试删除 local_path
        if skill.local_paths.is_none() || skill.local_paths.as_ref().unwrap().is_empty() {
            if let Some(local_path) = &skill.local_path {
                let path = PathBuf::from(local_path);
                if link_fs::is_dir_link(&path) {
                    if let Err(e) = link_fs::remove_dir_link(&path) {
                        errors.push(format!("无法删除技能链接: {}", e));
                    }
                } else if path.exists() {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            errors.push(format!("无法删除技能目录: {}", e));
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        errors.push(format!("无法删除技能文件: {}", e));
                    }
                }
            }
        }

        // 更新数据库
        skill.installed = false;
        skill.installed_at = None;
        skill.local_path = None;
        skill.local_paths = None;
        skill.source_path = None;
        skill.linked_tools = Vec::new();
        skill.is_local_only = false;

        self.db.save_skill(&skill).context("更新数据库失败")?;

        if errors.is_empty() {
            log::info!("Skill uninstalled successfully: {}", skill.name);
            self.invalidate_installed_cache();
            Ok(())
        } else {
            log::warn!("Skill uninstall completed with errors: {}", skill.name);
            self.invalidate_installed_cache();
            anyhow::bail!("UNINSTALL_PARTIAL_FAILURE: {}", errors.join("; "))
        }
    }

    /// 卸载特定路径的技能
    pub fn uninstall_skill_path(&self, skill_id: &str, path_to_remove: &str) -> Result<()> {
        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 删除指定路径的文件：链接用 remove_dir_link，真实目录用 remove_dir_all
        let mut errors: Vec<String> = Vec::new();
        let path = PathBuf::from(path_to_remove);
        if link_fs::is_dir_link(&path) {
            if let Err(e) = link_fs::remove_dir_link(&path) {
                errors.push(format!("无法删除技能链接: {}", e));
            }
        } else if path.exists() {
            if path.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    errors.push(format!("无法删除技能目录: {}", e));
                }
            } else if let Err(e) = std::fs::remove_file(&path) {
                errors.push(format!("无法删除技能文件: {}", e));
            }
        }

        // 从 local_paths 中移除该路径（使用规范化比较，兼容 Windows 大小写/分隔符差异）
        let normalized_remove = normalize_path_for_compare(&path);
        if let Some(mut paths) = skill.local_paths.clone() {
            paths.retain(|p| normalize_path_for_compare(&PathBuf::from(p)) != normalized_remove);

            if paths.is_empty() {
                // 如果没有剩余路径
                if skill.is_local_only && skill.repository_url == LOCAL_REPOSITORY_URL {
                    // 本地技能：直接删除 DB 记录（与 reconcile_stale_skills 行为一致）
                    self.db.delete_skill(&skill.id).context("删除技能记录失败")?;
                    log::info!("已删除本地技能记录: {}", skill.name);
                    return Ok(());
                }
                // 市场技能：标记为未安装
                skill.installed = false;
                skill.installed_at = None;
                skill.local_path = None;
                skill.local_paths = None;
            } else {
                // 还有其他路径,更新列表
                skill.local_paths = Some(paths.clone());
                skill.local_path = paths.last().cloned(); // 更新为最后一个路径
            }
        }

        self.db.save_skill(&skill).context("更新数据库失败")?;

        if errors.is_empty() {
            log::info!(
                "Skill path uninstalled: {} from {}",
                skill.name,
                path_to_remove
            );
            self.invalidate_installed_cache();
            Ok(())
        } else {
            log::warn!(
                "Skill path uninstall completed with errors: {} from {}",
                skill.name,
                path_to_remove
            );
            self.invalidate_installed_cache();
            anyhow::bail!("UNINSTALL_PATH_PARTIAL_FAILURE: {}", errors.join("; "))
        }
    }

    /// 获取所有 skills
    pub fn get_all_skills(&self) -> Result<Vec<Skill>> {
        self.db.get_skills()
    }

    /// 获取已安装的 skills
    pub fn get_installed_skills(&self) -> Result<Vec<Skill>> {
        // Check memory cache first (5-second TTL)
        if let Ok(cache) = self.installed_cache.lock() {
            if let Some((ts, skills)) = cache.as_ref() {
                if ts.elapsed().as_secs() < 5 {
                    return Ok(skills.clone());
                }
            }
        }

        // This intentionally checks the filesystem on demand instead of using a long-lived
        // backend cache: external tools or previous app versions may create/remove links while
        // the app is open. DB writes are still gated by installed_tool_state_changed.

        // 清理残留的 __cache__: / __staging__: 临时路径（仅首次调用时执行）
        if !self.cleanup_done.load(Ordering::Relaxed) {
            if let Err(e) = self.cleanup_stale_prepare_paths() {
                log::warn!("清理残留临时路径时出错（忽略）: {}", e);
            }
            self.cleanup_done.store(true, Ordering::Relaxed);
        }

        let tool_dirs = default_tool_dirs();
        let skills = self.db.get_skills()?;
        let mut installed = Vec::new();

        for skill in skills.into_iter().filter(|s| s.installed) {
            let refreshed = refresh_existing_tool_links_for_skill(&skill, &tool_dirs);
            if installed_tool_state_changed(&skill, &refreshed) {
                self.db.save_skill(&refreshed)?;
            }
            installed.push(refreshed);
        }

        if let Ok(mut cache) = self.installed_cache.lock() {
            *cache = Some((std::time::Instant::now(), installed.clone()));
        }

        Ok(installed)
    }

    fn invalidate_installed_cache(&self) {
        if let Ok(mut cache) = self.installed_cache.lock() {
            *cache = None;
        }
    }

    fn refresh_installed_tool_links(&self) -> Result<usize> {
        let tool_dirs = default_tool_dirs();
        let skills = self.db.get_skills()?;
        let mut refreshed_count = 0usize;

        for skill in skills.into_iter().filter(|s| s.installed) {
            let refreshed = refresh_existing_tool_links_for_skill(&skill, &tool_dirs);
            if installed_tool_state_changed(&skill, &refreshed) {
                self.db.save_skill(&refreshed)?;
                refreshed_count += 1;
            }
        }

        Ok(refreshed_count)
    }

    /// 扫描所有工具 skill 目录，导入未追踪的技能；通过 realpath 去重，避免链接和源重复导入
    pub fn scan_local_skills(&self) -> Result<Vec<Skill>> {
        use std::collections::HashSet;

        // 所有扫描到的技能
        let mut scanned_skills = Vec::new();
        // 新导入的技能（用于日志）
        let mut imported_skills = Vec::new();
        // realpath -> scanned_skills 索引，用于去重并关联多工具
        let mut seen_real_paths: HashMap<PathBuf, usize> = HashMap::new();

        // 获取当前数据库中的所有技能（用于去重和提取路径）
        let existing_skills = self.db.get_skills()?;

        // 1. 获取所有 unique 的 local_path 父目录
        let mut scan_dirs: HashSet<PathBuf> = HashSet::new();

        // 从已安装技能的 local_path 提取父目录（跳过临时缓存/staging路径）
        for skill in &existing_skills {
            if let Some(local_path) = &skill.local_path {
                if local_path.starts_with("__cache__:") || local_path.starts_with("__staging__:") {
                    continue;
                }
                if let Some(parent) = PathBuf::from(local_path).parent() {
                    scan_dirs.insert(parent.to_path_buf());
                }
            }
        }

        // 2. 添加所有工具的 skill 目录（确保始终扫描）
        let mut dir_to_tool: HashMap<PathBuf, String> = HashMap::new();
        for tool in AgentTool::all() {
            if let Some(dir) = tool.default_skills_dir() {
                dir_to_tool.insert(dir.clone(), tool.id().to_string());
                scan_dirs.insert(dir);
            }
        }

        log::info!("Will scan {} directories for local skills", scan_dirs.len());

        // 3. 扫描所有目录
        for scan_dir in scan_dirs {
            let scan_dir_tool = find_tool_id_for_scan_dir(&scan_dir, &dir_to_tool);
            if !scan_dir.exists() {
                log::debug!("Skipping non-existent directory: {:?}", scan_dir);
                continue;
            }

            log::info!("Scanning directory: {:?}", scan_dir);

            // 遍历技能目录
            if let Ok(entries) = std::fs::read_dir(&scan_dir) {
                'entry: for entry in entries.flatten() {
                    let path = entry.path();

                    // 只处理目录
                    if !path.is_dir() {
                        continue;
                    }

                    // 检查是否包含 SKILL.md
                    let skill_md_path = path.join("SKILL.md");
                    if !skill_md_path.exists() {
                        continue;
                    }

                    // 通过 realpath 去重：同一技能出现在多个工具目录时，关联所有工具
                    let real_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                    let path_str = path.to_string_lossy().to_string();
                    if let Some(&idx) = seen_real_paths.get(&real_path) {
                        // 同一技能已在本次扫描中处理过，将当前目录的工具关联上去
                        let existing: &mut Skill = &mut scanned_skills[idx];
                        if let Some(ref tool_id) = scan_dir_tool {
                            push_unique_value(&mut existing.linked_tools, tool_id.clone());
                            log::info!(
                                "关联工具 '{}' 到技能 '{}'（来自 {:?}）",
                                tool_id,
                                existing.name,
                                path
                            );
                        }
                        let paths = existing.local_paths.get_or_insert_with(Vec::new);
                        if !paths.contains(&path_str) {
                            paths.push(path_str.clone());
                        }
                        let mut should_save = true;
                        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
                            let (skill_name, skill_description) =
                                self.parse_frontmatter(&content).unwrap_or_else(|_| {
                                    (existing.name.clone(), existing.description.clone())
                                });
                            if existing.repository_url == LOCAL_REPOSITORY_URL {
                                existing.name = skill_name;
                                existing.description = skill_description;
                                existing.file_path = path_str.clone();
                            }
                        } else if scan_dir_tool.is_none() {
                            should_save = false;
                        }
                        if should_save {
                            let _ = self.db.save_skill(existing);
                        }
                        continue;
                    }
                    // 也检查 DB 中已有记录（前次扫描导入的）
                    // 找到匹配后必须 continue 'entry，否则会继续走新 skill 创建逻辑
                    if let Some(ref tool_id) = scan_dir_tool {
                        for db_skill in &existing_skills {
                            if !db_skill.installed {
                                continue;
                            }
                            let db_real = db_skill
                                .local_path
                                .as_deref()
                                .and_then(|p| std::fs::canonicalize(p).ok());
                            if db_real.as_deref() == Some(&real_path) {
                                let mut updated = db_skill.clone();
                                push_unique_value(&mut updated.linked_tools, tool_id.clone());
                                log::info!(
                                    "关联工具 '{}' 到已有技能 '{}'（来自 {:?}）",
                                    tool_id,
                                    updated.name,
                                    path
                                );
                                if let Some(ref mut paths) = updated.local_paths {
                                    if !paths.contains(&path_str) {
                                        paths.push(path_str.clone());
                                    }
                                } else {
                                    updated.local_paths = Some(vec![path_str.clone()]);
                                }
                                if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
                                    let (skill_name, skill_description) =
                                        self.parse_frontmatter(&content).unwrap_or_else(|_| {
                                            (updated.name.clone(), updated.description.clone())
                                        });
                                    if updated.repository_url == LOCAL_REPOSITORY_URL {
                                        updated.name = skill_name;
                                        updated.description = skill_description;
                                        updated.file_path = path_str.clone();
                                    }
                                    let checksum =
                                        self.scanner.calculate_checksum(content.as_bytes());
                                    if updated.checksum.as_deref() != Some(checksum.as_str()) {
                                        updated.checksum = Some(checksum);
                                    }
                                    log::info!(
                                        "刷新已有本地技能 '{}' 的元数据（来自 {:?}）",
                                        updated.name,
                                        skill_md_path
                                    );
                                }
                                let _ = self.db.save_skill(&updated);
                                scanned_skills.push(updated);
                                seen_real_paths.insert(real_path, scanned_skills.len() - 1);
                                // 跳过新 skill 创建逻辑，该路径已在 DB 中记录
                                continue 'entry;
                            }
                        }
                    }
                    seen_real_paths.insert(real_path, scanned_skills.len());

                    // 读取 SKILL.md 内容
                    match std::fs::read_to_string(&skill_md_path) {
                        Ok(content) => {
                            // 计算 checksum
                            let checksum = self.scanner.calculate_checksum(content.as_bytes());

                            // 解析 frontmatter 获取元数据（用于展示/更新）
                            let (skill_name, skill_description) =
                                self.parse_frontmatter(&content).unwrap_or_else(|_| {
                                    (
                                        path.file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string(),
                                        None,
                                    )
                                });

                            // 检查是否已存在（按 local_path 和 local_paths 去重，避免目录不变但名称变化导致重复导入）
                            let local_path_str = path.to_string_lossy().to_string();
                            let existing_by_path = existing_skills
                                .iter()
                                .filter(|s| {
                                    s.local_path.as_deref() == Some(local_path_str.as_str())
                                        || s.local_paths
                                            .as_ref()
                                            .map_or(false, |paths| paths.contains(&local_path_str))
                                })
                                .cloned()
                                .collect::<Vec<_>>();

                            if existing_by_path.len() > 1 {
                                log::warn!(
                                "Found {} duplicated skills with same local_path={}, will update the first one",
                                existing_by_path.len(),
                                local_path_str
                            );
                            }

                            if let Some(mut existing_skill) = existing_by_path.into_iter().next() {
                                // 确保安装状态/路径一致
                                if !existing_skill.installed {
                                    existing_skill.installed = true;
                                    existing_skill.installed_at = Some(Utc::now());
                                }
                                if existing_skill.local_path.as_deref()
                                    != Some(local_path_str.as_str())
                                {
                                    existing_skill.local_path = Some(local_path_str.clone());
                                }

                                // 关联当前目录对应的工具
                                if let Some(ref tool_id) = scan_dir_tool {
                                    if !existing_skill.linked_tools.contains(tool_id) {
                                        existing_skill.linked_tools.push(tool_id.clone());
                                    }
                                }

                                // 更新 checksum（基于 SKILL.md 内容）
                                let checksum_changed =
                                    existing_skill.checksum.as_deref() != Some(checksum.as_str());
                                if checksum_changed {
                                    existing_skill.checksum = Some(checksum.clone());
                                }

                                // 仅对本地导入的技能（repository_url == local）更新 name/description/file_path
                                // 避免覆盖市场技能的元数据来源（仓库扫描/市场配置）
                                if existing_skill.repository_url == LOCAL_REPOSITORY_URL {
                                    existing_skill.name = skill_name;
                                    existing_skill.description = skill_description;
                                    existing_skill.file_path = local_path_str.clone();
                                }

                                // 仅在 checksum 变化时重新扫描，避免每次扫描全量安全检查的性能开销
                                if checksum_changed {
                                    let locale = rust_i18n::locale();
                                    let report = self.scanner.scan_directory_with_options(
                                        path.to_str().unwrap_or(""),
                                        &existing_skill.id,
                                        &locale,
                                        ScanOptions { skip_readme: true },
                                        None,
                                    )?;

                                    Self::apply_scan_report(&mut existing_skill, &report);
                                }

                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // 二次匹配：按目录名为 local::* 技能查找已有记录，避免技能被移动+编辑后产生重复
                            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                            let existing_by_dir = existing_skills.iter().find(|s| {
                                s.id.starts_with("local::")
                                    && !s.installed
                                    && PathBuf::from(s.local_path.as_deref().unwrap_or(""))
                                        .file_name()
                                        .map(|n| n.to_string_lossy() == dir_name)
                                        .unwrap_or(false)
                            });
                            if let Some(mut existing_skill) = existing_by_dir.cloned() {
                                log::info!(
                                    "Reusing existing local skill '{}' for moved directory {:?} -> {:?}",
                                    existing_skill.id,
                                    existing_skill.local_path,
                                    path
                                );
                                let checksum_changed = existing_skill.checksum.as_deref() != Some(checksum.as_str());
                                existing_skill.local_path = Some(local_path_str.clone());
                                existing_skill.local_paths = Some(vec![local_path_str.clone()]);
                                existing_skill.source_path = Some(local_path_str.clone());
                                existing_skill.installed = true;
                                existing_skill.installed_at = Some(Utc::now());
                                existing_skill.name = skill_name;
                                existing_skill.description = skill_description;
                                existing_skill.file_path = local_path_str.clone();

                                if checksum_changed {
                                    existing_skill.checksum = Some(checksum);
                                    let locale = rust_i18n::locale();
                                    let report = self.scanner.scan_directory_with_options(
                                        path.to_str().unwrap_or(""),
                                        &existing_skill.id,
                                        &locale,
                                        ScanOptions { skip_readme: true },
                                        None,
                                    )?;
                                    Self::apply_scan_report(&mut existing_skill, &report);
                                }

                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // Fallback: match by checksum for renamed directories
                            let existing_by_checksum = existing_skills.iter().find(|s| {
                                s.checksum.as_deref() == Some(checksum.as_str())
                                    && !s.installed
                                    && s.repository_url == "local"
                            });
                            if let Some(mut existing_skill) = existing_by_checksum.cloned() {
                                log::info!(
                                    "Reusing existing local skill '{}' by checksum for renamed directory {:?} -> {:?}",
                                    existing_skill.id,
                                    existing_skill.local_path,
                                    path
                                );
                                existing_skill.local_path = Some(local_path_str.clone());
                                existing_skill.local_paths = Some(vec![local_path_str.clone()]);
                                existing_skill.source_path = Some(local_path_str.clone());
                                existing_skill.installed = true;
                                existing_skill.installed_at = Some(Utc::now());
                                existing_skill.checksum = Some(checksum);
                                existing_skill.name = skill_name;
                                existing_skill.description = skill_description;
                                existing_skill.file_path = local_path_str.clone();

                                let locale = rust_i18n::locale();
                                let report = self.scanner.scan_directory_with_options(
                                    path.to_str().unwrap_or(""),
                                    &existing_skill.id,
                                    &locale,
                                    ScanOptions { skip_readme: true },
                                    None,
                                )?;
                                Self::apply_scan_report(&mut existing_skill, &report);

                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // 生成技能 ID
                            let skill_id = build_local_skill_id(&checksum, &path);

                            // 扫描整个技能目录
                            let locale = rust_i18n::locale();
                            let report = self.scanner.scan_directory_with_options(
                                path.to_str().unwrap_or(""),
                                &skill_id,
                                &locale,
                                ScanOptions { skip_readme: true },
                                None,
                            )?;

                            log::info!(
                                "Scanned local skill '{}': score={}, files={:?}",
                                skill_name,
                                report.score,
                                report.scanned_files
                            );

                            // 创建 skill 对象（使用之前解析的元数据）
                            let local_path_str = path.to_string_lossy().to_string();
                            let linked_tools = scan_dir_tool.iter().cloned().collect::<Vec<_>>();
                            let skill = Skill {
                                id: skill_id,
                                name: skill_name,
                                description: skill_description,
                                repository_url: LOCAL_REPOSITORY_URL.to_string(),
                                repository_owner: Some(LOCAL_REPOSITORY_URL.to_string()),
                                file_path: path.to_string_lossy().to_string(),
                                version: None,
                                author: None,
                                installed: true,
                                installed_at: Some(Utc::now()),
                                local_path: Some(local_path_str.clone()),
                                local_paths: Some(vec![local_path_str.clone()]),
                                checksum: Some(checksum),
                                security_score: Some(report.score),
                                security_issues: Some(report.issues.clone()),
                                security_level: Some(report.level.as_str().to_string()),
                                security_report: Some(report.clone()),
                                scanned_at: Some(Utc::now()),
                                installed_commit_sha: None,
                                source_path: Some(local_path_str),
                                linked_tools,
                                is_local_only: true,
                            };

                            // 保存到数据库
                            self.db.save_skill(&skill)?;
                            imported_skills.push(skill.clone());
                            scanned_skills.push(skill);

                            log::info!("Imported local skill: {:?}", path);
                        }
                        Err(e) => {
                            log::warn!("Failed to read skill file {:?}: {}", skill_md_path, e);
                        }
                    }
                }
            }
        }

        // Reconciliation pass: detect skills that exist in DB as installed
        // but whose directories no longer exist on disk
        let stale_count = self.reconcile_stale_skills(&existing_skills)?;
        let refreshed_count = self.refresh_installed_tool_links()?;

        log::info!(
            "Scanned {} local skills, imported {} new skills, reconciled {} stale skills, refreshed {} tool-link states",
            scanned_skills.len(),
            imported_skills.len(),
            stale_count,
            refreshed_count
        );
        Ok(scanned_skills)
    }

    /// Reconcile DB state with filesystem: detect installed skills whose directories
    /// have been deleted externally and update DB accordingly.
    ///
    /// - For local-only skills with no remaining paths: delete the DB record entirely
    /// - For marketplace skills with no remaining paths: mark as uninstalled
    /// - For skills with some paths gone: prune stale paths and update linked_tools
    fn reconcile_stale_skills(&self, existing_skills: &[Skill]) -> Result<usize> {
        let mut stale_count = 0usize;

        for skill in existing_skills {
            if !skill.installed {
                continue;
            }

            let paths = match &skill.local_paths {
                Some(p) if !p.is_empty() => p.clone(),
                _ => match &skill.local_path {
                    Some(p) => vec![p.clone()],
                    None => continue,
                },
            };

            // Check which paths still exist on disk
            let alive_paths: Vec<String> = paths
                .iter()
                .filter(|p| PathBuf::from(p).exists())
                .cloned()
                .collect();

            if alive_paths.len() == paths.len() {
                continue; // All paths still exist, nothing to do
            }

            stale_count += 1;

            if alive_paths.is_empty() {
                // All paths are gone
                if skill.is_local_only {
                    log::info!(
                        "Deleting local-only skill '{}' (id={}): all paths removed from disk",
                        skill.name,
                        skill.id
                    );
                    if let Err(e) = self.db.delete_skill(&skill.id) {
                        log::warn!("Failed to delete stale local skill '{}': {}", skill.name, e);
                    }
                } else {
                    log::info!(
                        "Marking skill '{}' (id={}) as uninstalled: all paths removed from disk",
                        skill.name,
                        skill.id
                    );
                    let mut updated = skill.clone();
                    updated.installed = false;
                    updated.installed_at = None;
                    updated.local_path = None;
                    updated.local_paths = None;
                    updated.source_path = None;
                    updated.linked_tools = Vec::new();
                    if let Err(e) = self.db.save_skill(&updated) {
                        log::warn!("Failed to update stale skill '{}': {}", skill.name, e);
                    }
                }
            } else {
                // Some paths are gone — prune stale paths and clean up linked tools
                log::info!(
                    "Pruning stale paths for skill '{}' (id={}): {:?} -> {:?}",
                    skill.name,
                    skill.id,
                    paths,
                    alive_paths
                );

                let mut updated = skill.clone();

                // Remove linked tools whose paths are gone
                let stale_paths: std::collections::HashSet<&str> = paths
                    .iter()
                    .filter(|p| !alive_paths.contains(p))
                    .map(|s| s.as_str())
                    .collect();
                updated.linked_tools.retain(|tool_id| {
                    if let Some(tool) = AgentTool::from_id(tool_id) {
                        if let Some(tool_dir) = tool.default_skills_dir() {
                            // Check if any stale path belongs to this tool
                            let is_stale = stale_paths.iter().any(|sp| {
                                path_is_inside_dir_resolving_links(&PathBuf::from(sp), &tool_dir)
                            });
                            if is_stale {
                                return false;
                            }
                            // Also check: if any local_path was under this tool's dir and is now stale
                            let skill_path_was_in_tool_dir = updated.local_paths.as_ref()
                                .map(|paths| paths.iter().any(|p| {
                                    let p_buf = PathBuf::from(p);
                                    path_is_inside_dir_resolving_links(&p_buf, &tool_dir)
                                }))
                                .unwrap_or(false);
                            let any_stale_in_tool_dir = stale_paths.iter().any(|sp| {
                                let sp_buf = PathBuf::from(sp);
                                normalize_path_for_compare(&sp_buf).starts_with(
                                    &normalize_path_for_compare(&tool_dir)
                                )
                            });
                            if skill_path_was_in_tool_dir && any_stale_in_tool_dir {
                                return false;
                            }
                        }
                    }
                    true
                });

                updated.local_paths = Some(alive_paths.clone());
                updated.local_path = Some(alive_paths[0].clone());
                if updated
                    .source_path
                    .as_ref()
                    .map_or(true, |sp| !PathBuf::from(sp).exists())
                {
                    updated.source_path = Some(alive_paths[0].clone());
                }

                if let Err(e) = self.db.save_skill(&updated) {
                    log::warn!("Failed to prune paths for skill '{}': {}", skill.name, e);
                }
            }
        }

        Ok(stale_count)
    }

    /// 解析 SKILL.md 的 frontmatter（使用 serde_yaml，支持多行 block scalar 等 YAML 语法）
    fn parse_frontmatter(&self, content: &str) -> Result<(String, Option<String>)> {
        self.github.parse_skill_frontmatter(content)
    }

    /// 检测本地文件是否被修改（与缓存中的版本比较）
    fn detect_local_modifications(
        &self,
        installed_dir: &PathBuf,
        cached_dir: &PathBuf,
    ) -> Result<Vec<String>> {
        use std::fs;

        let mut modified_files = Vec::new();

        // 遍历已安装目录中的所有文件
        for entry in walkdir::WalkDir::new(installed_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let installed_file = entry.path();

                // 计算相对路径
                let relative_path = installed_file
                    .strip_prefix(installed_dir)
                    .context("无法计算相对路径")?;

                // 对应的缓存文件路径
                let cached_file = cached_dir.join(relative_path);

                // 如果缓存中没有该文件，说明是用户新增的
                if !cached_file.exists() {
                    modified_files.push(format!("新增: {}", relative_path.display()));
                    continue;
                }

                // 比较文件内容
                let installed_content = fs::read(installed_file)?;
                let cached_content = fs::read(&cached_file)?;

                if installed_content != cached_content {
                    modified_files.push(format!("修改: {}", relative_path.display()));
                }
            }
        }

        Ok(modified_files)
    }

    /// 准备技能更新：下载最新版本到临时目录并扫描，检测本地修改
    pub async fn prepare_skill_update(
        &self,
        skill_id: &str,
        locale: &str,
    ) -> Result<(crate::models::security::SecurityReport, Vec<String>)> {
        use anyhow::Context;

        log::info!("Preparing update for skill: {}", skill_id);

        // 获取技能信息
        let skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        if !skill.installed {
            anyhow::bail!("SKILL_NOT_INSTALLED");
        }

        // 获取仓库记录
        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("未找到对应的仓库记录")?
            .clone();

        // 重新下载仓库到新的临时缓存（staging）
        log::info!("下载最新版本到 staging 目录");
        let (owner, repo_name) = crate::models::Repository::from_github_url(&skill.repository_url)?;

        let staging_base_dir = dirs::cache_dir()
            .context("无法获取系统缓存目录")?
            .join("agent-skills-guard")
            .join("staging");

        // 清理旧的 staging 目录（如果存在）
        let staging_repo_dir = staging_base_dir.join(format!("{}_{}", owner, repo_name));
        if staging_repo_dir.exists() {
            std::fs::remove_dir_all(&staging_repo_dir)?;
        }

        // 下载最新版本
        let (extract_dir, new_commit_sha) = self
            .github
            .download_repository_archive(&owner, &repo_name, &staging_base_dir)
            .await
            .context("下载最新版本失败")?;

        log::info!("下载完成，最新 commit: {}", new_commit_sha);

        // 定位 staging 中的技能目录
        let staging_skill_dir =
            self.locate_skill_in_cache(extract_dir.as_path(), &skill.file_path)?;

        // 扫描最新版本
        let scan_report = self.scanner.scan_directory_with_options(
            staging_skill_dir.to_str().context("技能目录路径无效")?,
            &skill.id,
            locale,
            ScanOptions { skip_readme: true },
            None,
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        // 检测本地修改
        let modified_files = if let Some(local_path) = &skill.local_path {
            let installed_dir = PathBuf::from(local_path);
            if installed_dir.exists() {
                // 获取当前缓存中的版本（用于比较）
                if let Some(cache_path) = &repo.cache_path {
                    let cache_path_buf = PathBuf::from(cache_path);
                    if cache_path_buf.exists() {
                        match self.locate_skill_in_cache(cache_path_buf.as_path(), &skill.file_path)
                        {
                            Ok(cached_skill_dir) => {
                                self.detect_local_modifications(&installed_dir, &cached_skill_dir)?
                            }
                            Err(e) => {
                                log::warn!("无法定位缓存中的技能目录: {}", e);
                                Vec::new()
                            }
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        log::info!("检测到 {} 个本地修改", modified_files.len());

        // 保存 staging 信息到数据库（临时）
        // 我们使用一个特殊的字段来标记这是 staging 路径
        let mut skill_update = skill.clone();
        Self::apply_scan_report(&mut skill_update, &scan_report);
        skill_update.local_path = Some(format!(
            "__staging__:{}",
            staging_skill_dir.to_string_lossy()
        ));

        self.db.save_skill(&skill_update)?;

        Ok((scan_report, modified_files))
    }

    /// 确认技能更新：从 staging 写入到安装目录，并在缓存目录保留备份
    pub fn confirm_skill_update(
        &self,
        skill_id: &str,
        force_overwrite: bool,
        allow_partial_scan: bool,
    ) -> Result<()> {
        use anyhow::Context;

        log::info!("Confirming update for skill: {}", skill_id);

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 获取 staging 路径
        let staging_marker = skill.local_path.as_ref().context("技能尚未准备更新")?;

        if !staging_marker.starts_with("__staging__:") {
            anyhow::bail!("SKILL_NOT_PREPARED_FOR_UPDATE");
        }

        let staging_path_str = &staging_marker[12..]; // 去掉 "__staging__:" 前缀
        let staging_dir = PathBuf::from(staging_path_str);

        if !staging_dir.exists() {
            anyhow::bail!("STAGING_DIR_NOT_FOUND");
        }

        // 获取原安装路径（从 local_paths）
        let install_paths = skill.local_paths.as_ref().context("无法获取安装路径")?;

        if install_paths.is_empty() {
            anyhow::bail!("SKILL_NO_INSTALL_PATH");
        }

        // 始终以当前活跃安装路径（local_path / local_paths 最后一个）为更新目标。
        // 若该路径是软链接，解析为真实路径：更新真实目录后所有指向它的软链接自动生效。
        let (display_install_dir, target_install_dir) = {
            let raw = skill
                .local_paths
                .as_ref()
                .and_then(|paths| paths.last())
                .map(PathBuf::from)
                .context("技能没有有效的活跃安装路径")?;

            let (display, real) = resolve_update_install_paths(&raw);
            if !paths_point_to_same_location(&display, &real) {
                log::info!(
                    "更新目标是目录链接，解析为真实路径: {:?} -> {:?}",
                    display,
                    real
                );
            }
            (display, real)
        };

        #[derive(Debug)]
        enum BackupDir {
            Renamed(PathBuf),
            Copied(PathBuf),
        }

        // 创建备份（如果目录存在）：优先移动到缓存目录；若移动失败则复制到缓存目录
        let backup_dir = if target_install_dir.exists() {
            let dir_name = target_install_dir
                .file_name()
                .context("无效的目录名")?
                .to_string_lossy();
            let backup_root = dirs::cache_dir()
                .context("无法获取系统缓存目录")?
                .join("agent-skills-guard")
                .join("skill-backups");

            std::fs::create_dir_all(&backup_root)
                .context(format!("无法创建备份缓存目录: {:?}", backup_root))?;

            let mut backup_path = backup_root.join(format!("{}.bak", dir_name));

            if backup_path.exists() {
                match std::fs::remove_dir_all(&backup_path) {
                    Ok(()) => {}
                    Err(remove_err) => {
                        if !force_overwrite {
                            return Err(anyhow::anyhow!(format!(
                                "无法删除旧备份目录（缓存目录）: {:?}\n错误: {}\n\n请检查该目录是否被其他程序占用",
                                backup_path, remove_err
                            )));
                        }

                        // 强制覆盖时，为了不中断流程，改用一个唯一的备份目录名
                        let epoch_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        backup_path = backup_root.join(format!("{}.bak-{}", dir_name, epoch_ms));
                        let _ = std::fs::remove_dir_all(&backup_path);
                    }
                }
            }

            // 尝试移动：移动成功意味着我们可以“干净地”写入新版本（更接近原子替换）
            match rename_with_retry(&target_install_dir, &backup_path) {
                Ok(()) => {
                    log::info!("创建备份(移动到缓存): {:?}", backup_path);
                    Some(BackupDir::Renamed(backup_path))
                }
                Err(move_err) => {
                    log::warn!(
                        "无法移动技能目录到缓存备份（将改用复制备份 + 原地覆盖）: {}",
                        move_err
                    );

                    match self.copy_dir_recursive(&target_install_dir, &backup_path, &mut 0) {
                        Ok(()) => {
                            log::info!("创建备份(复制到缓存): {:?}", backup_path);
                            Some(BackupDir::Copied(backup_path))
                        }
                        Err(copy_err) => {
                            if !force_overwrite {
                                return Err(anyhow::anyhow!(format!(
                                    "无法为更新创建备份（缓存目录）\n目标: {:?}\n备份: {:?}\n\n复制备份错误: {}\n\n提示：你可以关闭正在使用该技能的程序后重试；或勾选“强制覆盖本地修改”继续（将无法保证可回滚）。",
                                    target_install_dir, backup_path, copy_err
                                )));
                            }

                            log::warn!(
                                "创建备份(复制到缓存)失败，将在无备份情况下继续: {}",
                                copy_err
                            );
                            None
                        }
                    }
                }
            }
        } else {
            None
        };

        // 确保目标父目录存在
        std::fs::create_dir_all(&target_install_dir.parent().context("无效的安装路径")?)?;

        let scan_report = self.rescan_skill_directory_for_confirmation(
            &staging_dir,
            &skill.id,
            allow_partial_scan,
        )?;

        // 确保目标目录干净：
        //   - rename备份成功：目标已不存在，创建新空目录
        //   - copy备份成功：备份已完成，安全清空原目录后重建
        //   - force_overwrite：无论是否有备份，强制清空
        let should_clear = matches!(backup_dir, Some(BackupDir::Copied(_))) || force_overwrite;
        if !target_install_dir.exists() {
            std::fs::create_dir_all(&target_install_dir)
                .context(format!("无法创建目标目录: {:?}", target_install_dir))?;
        } else if should_clear {
            if let Err(clear_err) = std::fs::remove_dir_all(&target_install_dir) {
                log::warn!(
                    "无法清空旧技能目录，将尝试直接覆盖写入（可能保留部分旧文件）: {}",
                    clear_err
                );
            } else {
                std::fs::create_dir_all(&target_install_dir)
                    .context(format!("无法重建目标目录: {:?}", target_install_dir))?;
            }
        }

        match self.copy_dir_recursive(&staging_dir, &target_install_dir, &mut 0) {
            Ok(_) => {
                log::info!("成功更新技能到: {:?}", target_install_dir);

                // 备份保留在缓存目录，便于必要时人工回滚；下一次更新会覆盖旧备份

                // 更新数据库：恢复 local_path，更新 installed_commit_sha
                skill.local_path = Some(display_install_dir.to_string_lossy().to_string());
                Self::apply_scan_report(&mut skill, &scan_report);

                // 从 staging 路径推导出 extracted 目录并提取 commit SHA
                // - staging_dir 指向 skill 目录（可能是仓库根目录或其子目录）
                // - extracted_dir 是 {cache}/.../extracted/，其下第一层目录名为 {owner}-{repo}-{sha}
                let extract_dir = {
                    let mut repo_root = staging_dir.clone();
                    if skill.file_path != "." {
                        let components_count = std::path::Path::new(&skill.file_path)
                            .components()
                            .filter(|c| matches!(c, std::path::Component::Normal(_)))
                            .count();

                        for _ in 0..components_count {
                            repo_root = repo_root
                                .parent()
                                .context("无效的 staging 路径：无法定位仓库根目录")?
                                .to_path_buf();
                        }
                    }

                    repo_root
                        .parent()
                        .context("无效的 staging 路径：无法定位 extracted 目录")?
                        .to_path_buf()
                };

                match self.github.extract_commit_sha_from_cache(&extract_dir) {
                    Ok(new_sha) => {
                        skill.installed_commit_sha = Some(new_sha.clone());
                        log::info!("更新 installed_commit_sha");

                        // 将 staging 下载的版本提升为“仓库缓存基线”，避免后续把已更新内容误判为“本地修改”
                        if let Ok((owner, repo_name)) =
                            crate::models::Repository::from_github_url(&skill.repository_url)
                        {
                            if let Some(cache_base_dir) = dirs::cache_dir() {
                                let repositories_base_dir = cache_base_dir
                                    .join("agent-skills-guard")
                                    .join("repositories");
                                let repo_cache_dir =
                                    repositories_base_dir.join(format!("{}_{}", owner, repo_name));
                                let extracted_dest = repo_cache_dir.join("extracted");

                                if let Err(e) = std::fs::create_dir_all(&repo_cache_dir) {
                                    log::warn!("无法创建仓库缓存目录，将跳过缓存同步: {}", e);
                                } else {
                                    if extracted_dest.exists() {
                                        let _ = std::fs::remove_dir_all(&extracted_dest);
                                    }

                                    match rename_with_retry(&extract_dir, &extracted_dest) {
                                        Ok(()) => {
                                            log::info!(
                                                "已同步仓库缓存(移动): {:?}",
                                                extracted_dest
                                            );
                                        }
                                        Err(rename_err) => {
                                            log::warn!(
                                                "无法移动 staging 缓存到仓库缓存，将尝试复制: {}",
                                                rename_err
                                            );
                                            if let Err(copy_err) =
                                                self.copy_dir_recursive(&extract_dir, &extracted_dest, &mut 0)
                                            {
                                                log::warn!("同步仓库缓存(复制)失败: {}", copy_err);
                                            } else {
                                                log::info!(
                                                    "已同步仓库缓存(复制): {:?}",
                                                    extracted_dest
                                                );
                                            }
                                        }
                                    }

                                    if extracted_dest.exists() {
                                        if let Ok(repositories) = self.db.get_repositories() {
                                            if let Some(repo) = repositories
                                                .iter()
                                                .find(|r| r.url == skill.repository_url)
                                            {
                                                let cache_path_str =
                                                    extracted_dest.to_string_lossy().to_string();
                                                if let Err(e) = self.db.update_repository_cache(
                                                    &repo.id,
                                                    &cache_path_str,
                                                    Utc::now(),
                                                    Some(&new_sha),
                                                ) {
                                                    log::warn!("更新仓库缓存信息失败: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("无法提取新的 commit SHA: {}", e);
                    }
                }

                skill.installed_at = Some(Utc::now());
                self.db.save_skill(&skill)?;

                log::info!("技能更新确认完成: {}", skill.name);
                Ok(())
            }
            Err(e) => {
                // 恢复备份
                if let Some(backup) = backup_dir {
                    if target_install_dir.exists() {
                        let _ = std::fs::remove_dir_all(&target_install_dir);
                    }

                    match backup {
                        BackupDir::Renamed(p) => {
                            let _ = std::fs::rename(&p, &target_install_dir);
                            log::warn!("更新失败，已恢复备份(重命名): {:?}", p);
                        }
                        BackupDir::Copied(p) => {
                            let _ = self.copy_dir_recursive(&p, &target_install_dir, &mut 0);
                            log::warn!("更新失败，已恢复备份(复制): {:?}", p);
                        }
                    }
                }
                Err(e)
            }
        }
    }

    /// 取消技能更新：清理 staging 目录
    pub fn cancel_skill_update(&self, skill_id: &str) -> Result<()> {
        use anyhow::Context;

        log::info!("Canceling update for skill: {}", skill_id);

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 获取 staging 路径
        let staging_marker = skill.local_path.as_ref().context("技能尚未准备更新")?;

        if !staging_marker.starts_with("__staging__:") {
            log::warn!("技能没有处于更新准备状态");
            return Ok(());
        }

        let staging_path_str = &staging_marker[12..];
        let staging_dir = PathBuf::from(staging_path_str);

        // 只删除该 skill 自身的 staging 目录，不上溯到父/祖父目录，
        // 避免误删同一仓库其他 skill 的 staging 数据。
        if staging_dir.exists() {
            std::fs::remove_dir_all(&staging_dir)?;
            log::info!("已删除 staging 目录: {:?}", staging_dir);
        }

        // 恢复数据库中的 local_path
        if let Some(local_paths) = &skill.local_paths {
            if let Some(last_path) = local_paths.last() {
                skill.local_path = Some(last_path.clone());
            } else {
                skill.local_path = None;
            }
        } else {
            skill.local_path = None;
        }

        self.db.save_skill(&skill)?;

        log::info!("技能更新已取消: {}", skill.name);
        Ok(())
    }

    /// 为指定 skill 重建到目标工具的链接
    /// - 本地技能先自动提升到通用目录（~/.agents/skills），原位置替换为链接
    /// - 再按当前 linked_tools 删除旧链接
    /// - 最后为 target_tools（不含 agents）创建新链接
    pub fn sync_skill_to_tools(&self, skill_id: &str, target_tools: Vec<String>) -> Result<()> {
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 本地技能自动提升：复制到通用目录，原位置替换为 Junction
        if skill.is_local_only {
            let original_path = skill
                .local_path
                .as_ref()
                .context("本地技能缺少 local_path")?
                .clone();
            let original = PathBuf::from(&original_path);
            let dir_name = original.file_name().context("无效的技能目录名")?;
            let common_dir = self.skills_dir.join(&dir_name);

            // 确保通用目录的父目录存在
            std::fs::create_dir_all(&self.skills_dir).context("无法创建通用技能目录")?;

            let already_in_common_dir = paths_point_to_same_location(&original, &common_dir);

            if already_in_common_dir {
                log::info!("本地技能已位于通用目录，无需复制: {:?}", common_dir);
            } else {
                // 复制到通用目录
                let mut files_copied = 0;
                self.copy_dir_recursive(&original, &common_dir, &mut files_copied)?;
                log::info!(
                    "已将本地技能复制到通用目录: {:?} ({} 个文件)",
                    common_dir,
                    files_copied
                );

                // 删除原目录，替换为指向通用目录的 Junction
                if link_fs::is_dir_link(&original) {
                    link_fs::remove_dir_link(&original)?;
                } else {
                    std::fs::remove_dir_all(&original).context("无法删除原技能目录")?;
                }
                link_fs::create_dir_link(&common_dir, &original)
                    .context("无法将原位置替换为链接")?;
            }

            // 更新技能记录
            let common_str = common_dir.to_string_lossy().to_string();
            skill.source_path = Some(common_str.clone());
            skill.local_path = Some(common_str.clone());
            skill.local_paths = if already_in_common_dir {
                Some(vec![common_str])
            } else {
                Some(vec![common_str, original_path.clone()])
            };
            skill.is_local_only = false;

            // 识别原路径所属工具并加入 linked_tools
            let mut linked = Vec::new();
            let original = PathBuf::from(&original_path);
            for tool in AgentTool::all() {
                if tool == AgentTool::Agents {
                    continue;
                }
                if let Some(tool_dir) = tool.default_skills_dir() {
                    if path_is_inside_dir_resolving_links(&original, &tool_dir) {
                        linked.push(tool.id().to_string());
                        break;
                    }
                }
            }
            skill.linked_tools = linked;
            self.db.save_skill(&skill)?;
        }

        let source_path = skill
            .source_path
            .as_ref()
            .or(skill.local_path.as_ref())
            .context("技能未安装（无 source_path）")?
            .clone();
        let source = PathBuf::from(&source_path);

        let skill_dir_name = source
            .file_name()
            .context("无效的技能目录名")?
            .to_string_lossy()
            .to_string();

        // 先创建新链接，再删除不再需要的旧链接（原子性：新链接创建失败不影响已有链接）
        let mut sync_errors: Vec<String> = Vec::new();
        for tool_id in &target_tools {
            let tool = match AgentTool::from_id(tool_id) {
                Some(t) if t != AgentTool::Agents => t,
                _ => continue,
            };
            if let Some(tool_dir) = tool.default_skills_dir() {
                let link = tool_dir.join(&skill_dir_name);
                if link.exists() || link_fs::is_dir_link(&link) {
                    if tool_skill_path_is_compatible_with_source(
                        &source,
                        &link,
                        skill.checksum.as_deref(),
                    ) {
                        log::info!("复用已存在的兼容工具路径 [{:?}]", link);
                    } else {
                        let msg =
                            format!("工具 '{}' 下已存在同名但内容不同的技能，不覆盖", tool.id());
                        log::warn!("{}", msg);
                        sync_errors.push(msg);
                    }
                    continue;
                }

                match link_fs::create_dir_link(&source, &link) {
                    Ok(()) => {}
                    Err(e) => {
                        let msg = format!("创建链接到工具 '{}' 失败: {}", tool.id(), e);
                        log::warn!("{}", msg);
                        sync_errors.push(msg);
                    }
                }
            }
        }

        // 删除旧链接（仅删除不在新 target_tools 中且确实指向自身源的链接）
        for old_tool_id in &skill.linked_tools {
            if target_tools.iter().any(|t| t == old_tool_id) {
                continue; // 仍在目标工具列表中，保留
            }
            if let Some(tool) = AgentTool::from_id(old_tool_id) {
                if let Some(tool_dir) = tool.default_skills_dir() {
                    let link = tool_dir.join(&skill_dir_name);
                    if link_fs::is_dir_link(&link) {
                        let targets_source = link_fs::read_dir_link_target(&link)
                            .map(|target| paths_point_to_same_location(&source, &target))
                            .unwrap_or(false);
                        if targets_source {
                            if let Err(e) = link_fs::remove_dir_link(&link) {
                                log::warn!("删除旧链接失败 [{:?}]: {}", link, e);
                            }
                        } else {
                            log::warn!(
                                "跳过删除旧链接 [{:?}]：链接目标不指向源目录，可能为第三方工具建立",
                                link
                            );
                        }
                    }
                }
            }
        }

        let reconciled = refresh_existing_tool_links_for_skill(&skill, &default_tool_dirs());
        self.db.save_skill(&reconciled)?;

        log::info!("Synced skill '{}' to tools: {:?}", skill.name, target_tools);
        self.invalidate_installed_cache();
        if sync_errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!("SYNC_PARTIAL_FAILURE: {}", sync_errors.join("; "))
        }
    }

    /// 为所有已安装 skill 批量同步到指定工具（含本地技能）
    pub fn sync_all_skills_to_tools(&self, target_tools: Vec<String>) -> Result<()> {
        let skills = self.db.get_skills()?;
        let managed: Vec<_> = skills.into_iter().filter(|s| s.installed).collect();

        let mut errors = Vec::new();
        for skill in managed {
            if let Err(e) = self.sync_skill_to_tools(&skill.id, target_tools.clone()) {
                log::warn!("同步 skill '{}' 失败: {}", skill.name, e);
                errors.push(format!("{}: {}", skill.name, e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!("SYNC_BATCH_PARTIAL_FAILURE:\n{}", errors.join("\n"))
        }
    }
}

fn is_retryable_rename_error(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }

    matches!(err.raw_os_error(), Some(5 | 32 | 33))
}

fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last_err: Option<std::io::Error> = None;
    let attempts = 8usize;

    for attempt in 0..attempts {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(err) => {
                let retryable = is_retryable_rename_error(&err);
                let is_last = attempt + 1 >= attempts;
                last_err = Some(err);
                if retryable && !is_last {
                    // 指数退避（上限 4s）：250ms, 500ms, 1000ms, 2000ms, 4000ms, 4000ms, 4000ms
                    // Windows 病毒扫描可能持有句柄 > 2 秒
                    let delay_ms = (250u64.saturating_mul(1u64 << attempt)).min(4000);
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    continue;
                }
                break;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::other("rename_with_retry failed")))
}

#[cfg(test)]
mod tests {
    use super::{
        build_local_skill_id, build_synced_tool_state, find_tool_id_for_scan_dir,
        paths_point_to_same_location, refresh_existing_tool_links_for_skill,
        resolve_update_install_paths, resolve_update_target_install_dir,
        restore_installation_backup, tool_skill_path_is_compatible_with_source,
    };
    use crate::services::{link_fs, Database};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn build_synced_tool_state_replaces_previous_links_with_requested_targets() {
        let source = PathBuf::from("/tmp/.agents/skills/example");
        let target_tools = vec!["codex".to_string()];

        let (linked_tools, local_paths) =
            build_synced_tool_state(&source, "example", &target_tools);

        assert_eq!(linked_tools, vec!["codex".to_string()]);
        assert_eq!(local_paths[0], "/tmp/.agents/skills/example");
        assert!(local_paths
            .iter()
            .map(|path| path.replace('\\', "/"))
            .any(|path| path.ends_with(".codex/skills/example")));
        assert!(!linked_tools.contains(&"claude-code".to_string()));
    }

    #[test]
    fn build_synced_tool_state_clears_links_when_no_targets_are_requested() {
        let source = PathBuf::from("/tmp/.agents/skills/example");

        let (linked_tools, local_paths) = build_synced_tool_state(&source, "example", &[]);

        assert!(linked_tools.is_empty());
        assert_eq!(local_paths, vec!["/tmp/.agents/skills/example".to_string()]);
    }

    #[test]
    fn local_skill_id_includes_path_not_only_checksum() {
        let checksum = "b81e2ff87ed8fa4d0b81e2ff87ed8fa4d";
        let agents_path = PathBuf::from("C:/Users/Bruce/.agents/skills/frontend-design");
        let claude_path = PathBuf::from("C:/Users/Bruce/.claude/skills/frontend-design");

        assert_ne!(
            build_local_skill_id(checksum, &agents_path),
            build_local_skill_id(checksum, &claude_path)
        );
    }

    #[test]
    fn same_path_detection_handles_already_promoted_local_skill() {
        let original = PathBuf::from("C:/Users/Bruce/.agents/skills/frontend-design");
        let common = PathBuf::from("C:/Users/Bruce/.agents/skills/frontend-design");

        assert!(paths_point_to_same_location(&original, &common));
    }

    #[test]
    fn update_target_resolution_follows_directory_links() {
        let temp = tempfile::tempdir().unwrap();
        let real_dir = temp.path().join("real-skill");
        let link_dir = temp.path().join("linked-skill");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::fs::write(real_dir.join("SKILL.md"), "test").unwrap();
        link_fs::create_dir_link(&real_dir, &link_dir).unwrap();

        let resolved = resolve_update_target_install_dir(&link_dir);

        assert!(
            paths_point_to_same_location(&resolved, &real_dir),
            "updates must write through directory links so every linked tool sees the new files"
        );
    }

    #[test]
    fn update_install_paths_preserve_display_path_for_links() {
        let temp = tempfile::tempdir().unwrap();
        let real_dir = temp.path().join("real-skill");
        let link_dir = temp.path().join("linked-skill");
        std::fs::create_dir_all(&real_dir).unwrap();
        link_fs::create_dir_link(&real_dir, &link_dir).unwrap();

        let (display_path, write_target) = resolve_update_install_paths(&link_dir);

        assert!(paths_point_to_same_location(&display_path, &link_dir));
        assert!(paths_point_to_same_location(&write_target, &real_dir));
    }

    #[test]
    fn scan_dir_tool_detection_follows_linked_tool_directory_targets() {
        let temp = tempfile::tempdir().unwrap();
        let real_tools_dir = temp.path().join("real-claude-skills");
        let linked_tools_dir = temp.path().join("home").join(".claude").join("skills");
        std::fs::create_dir_all(&real_tools_dir).unwrap();
        link_fs::create_dir_link(&real_tools_dir, &linked_tools_dir).unwrap();

        let mut dir_to_tool = HashMap::new();
        dir_to_tool.insert(linked_tools_dir, "claude-code".to_string());

        assert_eq!(
            find_tool_id_for_scan_dir(&real_tools_dir, &dir_to_tool).as_deref(),
            Some("claude-code")
        );
    }

    #[test]
    fn installed_skill_state_detects_existing_tool_directory_links() {
        let temp = tempfile::tempdir().unwrap();
        let agents_dir = temp.path().join(".agents").join("skills");
        let claude_dir = temp.path().join(".claude").join("skills");
        let source = agents_dir.join("frontend-design");
        let claude_link = claude_dir.join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "test").unwrap();
        link_fs::create_dir_link(&source, &claude_link).unwrap();

        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = true;
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.linked_tools = Vec::new();

        let tool_dirs = vec![
            (agents_dir, "agents".to_string()),
            (claude_dir, "claude-code".to_string()),
        ];

        let refreshed = refresh_existing_tool_links_for_skill(&skill, &tool_dirs);

        assert!(refreshed.linked_tools.contains(&"claude-code".to_string()));
        assert!(refreshed
            .local_paths
            .as_ref()
            .unwrap()
            .iter()
            .any(|path| path
                .replace('\\', "/")
                .ends_with(".claude/skills/frontend-design")));
    }

    #[test]
    fn installed_skill_state_prunes_stale_agents_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = codex_dir.join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "test").unwrap();

        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = true;
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.linked_tools = vec!["agents".to_string(), "codex".to_string()];

        let refreshed =
            refresh_existing_tool_links_for_skill(&skill, &[(codex_dir, "codex".to_string())]);

        assert!(!refreshed.linked_tools.contains(&"agents".to_string()));
        assert!(refreshed.linked_tools.contains(&"codex".to_string()));
    }

    #[test]
    fn installed_skill_state_adopts_existing_tool_directory_with_same_checksum() {
        let temp = tempfile::tempdir().unwrap();
        let agents_dir = temp.path().join(".agents").join("skills");
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = agents_dir.join("frontend-design");
        let codex_existing = codex_dir.join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&codex_existing).unwrap();
        std::fs::write(source.join("SKILL.md"), "same skill").unwrap();
        std::fs::write(codex_existing.join("SKILL.md"), "same skill").unwrap();

        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(source.to_string_lossy().to_string());
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.checksum =
            Some(crate::security::SecurityScanner::new().calculate_checksum(b"same skill"));
        skill.linked_tools = Vec::new();

        let refreshed = refresh_existing_tool_links_for_skill(
            &skill,
            &[(codex_dir.clone(), "codex".to_string())],
        );

        assert!(refreshed.linked_tools.contains(&"codex".to_string()));
        assert!(refreshed
            .local_paths
            .as_ref()
            .unwrap()
            .contains(&codex_existing.to_string_lossy().to_string()));
    }

    #[test]
    fn installed_skill_state_drops_missing_agents_source_when_compatible_tool_path_exists() {
        let temp = tempfile::tempdir().unwrap();
        let missing_agents_source = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("frontend-design");
        let codex_dir = temp.path().join(".codex").join("skills");
        let codex_existing = codex_dir.join("frontend-design");
        std::fs::create_dir_all(&codex_existing).unwrap();
        std::fs::write(codex_existing.join("SKILL.md"), "same skill").unwrap();

        let checksum = crate::security::SecurityScanner::new().calculate_checksum(b"same skill");
        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            missing_agents_source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(missing_agents_source.to_string_lossy().to_string());
        skill.local_path = Some(missing_agents_source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![missing_agents_source.to_string_lossy().to_string()]);
        skill.checksum = Some(checksum);
        skill.linked_tools = Vec::new();

        let refreshed =
            refresh_existing_tool_links_for_skill(&skill, &[(codex_dir, "codex".to_string())]);

        let codex_path = codex_existing.to_string_lossy().to_string();
        assert_eq!(refreshed.source_path.as_deref(), Some(codex_path.as_str()));
        assert_eq!(refreshed.local_path.as_deref(), Some(codex_path.as_str()));
        assert_eq!(refreshed.local_paths.as_deref(), Some(&[codex_path][..]));
        assert!(refreshed.linked_tools.contains(&"codex".to_string()));
    }

    #[test]
    fn tool_skill_path_is_not_compatible_when_same_name_has_different_checksum() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("frontend-design");
        let target = temp
            .path()
            .join(".codex")
            .join("skills")
            .join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("SKILL.md"), "source skill").unwrap();
        std::fs::write(target.join("SKILL.md"), "different skill").unwrap();

        let source_checksum =
            crate::security::SecurityScanner::new().calculate_checksum(b"source skill");

        assert!(!tool_skill_path_is_compatible_with_source(
            &source,
            &target,
            Some(source_checksum.as_str())
        ));
    }

    #[test]
    fn copy_dir_recursive_creates_destination_root_before_copying_files() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(db);
        let src = temp.path().join("source");
        let dst = temp.path().join("missing-destination");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join(".gitignore"), "target\n").unwrap();

        let mut copied = 0;
        manager.copy_dir_recursive(&src, &dst, &mut copied).unwrap();

        assert_eq!(copied, 1);
        assert_eq!(
            std::fs::read_to_string(dst.join(".gitignore")).unwrap(),
            "target\n"
        );
    }

    #[test]
    fn restore_installation_backup_replaces_partial_target_directory() {
        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join(".example.backup-test");
        let final_dir = temp.path().join("example");

        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("SKILL.md"), "original").unwrap();
        std::fs::create_dir_all(&final_dir).unwrap();
        std::fs::write(final_dir.join("partial.txt"), "partial").unwrap();

        restore_installation_backup(&backup, &final_dir).unwrap();

        assert!(!backup.exists());
        assert_eq!(
            std::fs::read_to_string(final_dir.join("SKILL.md")).unwrap(),
            "original"
        );
        assert!(!final_dir.join("partial.txt").exists());
    }
}
