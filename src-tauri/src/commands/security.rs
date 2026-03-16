use crate::commands::AppState;
use crate::i18n::validate_locale;
use crate::models::security::{SecurityLevel, SecurityReport, SkillScanResult};
use crate::models::Skill;
use crate::security::{ScanOptions, SecurityScanner};
use anyhow::Result;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use rust_i18n::t;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, Clone)]
struct ScanProgressEvent {
    scan_id: String,
    kind: String,
    item_id: String,
    file_path: String,
}

const DEFAULT_SCAN_PARALLELISM: usize = 3;
const MAX_SCAN_PARALLELISM: usize = 8;

fn clamp_scan_parallelism(scan_parallelism: Option<usize>) -> usize {
    scan_parallelism
        .unwrap_or(DEFAULT_SCAN_PARALLELISM)
        .clamp(1, MAX_SCAN_PARALLELISM)
}

/// 扫描所有已安装的 skills
#[tauri::command]
pub async fn scan_all_installed_skills(
    state: State<'_, AppState>,
    locale: String,
    scan_parallelism: Option<usize>,
) -> Result<Vec<SkillScanResult>, String> {
    let locale = validate_locale(&locale);
    let skills = state.db.get_skills().map_err(|e| e.to_string())?;
    let installed_skills: Vec<Skill> = skills
        .into_iter()
        .filter(|s| s.installed && s.local_path.is_some())
        .collect();

    let parallelism = clamp_scan_parallelism(scan_parallelism);
    let db = state.db.clone();
    let locale_owned = locale.to_string();

    let pool = ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .build()
        .map_err(|e| e.to_string())?;

    let mut results = pool.install(|| {
        installed_skills
            .par_iter()
            .enumerate()
            .filter_map(|(index, skill)| {
                let Some(local_path) = &skill.local_path else {
                    return None;
                };
                let path = PathBuf::from(local_path);
                if !path.exists() || !path.is_dir() {
                    log::warn!("Skill directory does not exist: {:?}", path);
                    return None;
                }

                let scanner = SecurityScanner::new();
                let report = match scanner.scan_directory_with_options(
                    path.to_str().unwrap_or(""),
                    &skill.id,
                    &locale_owned,
                    ScanOptions { skip_readme: true },
                    None,
                ) {
                    Ok(report) => report,
                    Err(e) => {
                        log::warn!("Failed to scan skill {}: {}", skill.name, e);
                        return None;
                    }
                };

                let mut updated = skill.clone();
                updated.security_score = Some(report.score);
                updated.security_level = Some(report.level.as_str().to_string());
                updated.security_issues = Some(
                    report
                        .issues
                        .iter()
                        .map(|i| {
                            let file_info = i
                                .file_path
                                .as_ref()
                                .map(|f| format!("[{}] ", f))
                                .unwrap_or_default();
                            format!("{}{:?}: {}", file_info, i.severity, i.description)
                        })
                        .collect(),
                );
                updated.scanned_at = Some(chrono::Utc::now());

                if let Err(e) = db.save_skill(&updated) {
                    log::warn!("Failed to save skill {}: {}", updated.name, e);
                }

                Some((
                    index,
                    SkillScanResult {
                        skill_id: updated.id.clone(),
                        skill_name: updated.name.clone(),
                        score: report.score,
                        level: report.level.as_str().to_string(),
                        scanned_at: chrono::Utc::now().to_rfc3339(),
                        report,
                    },
                ))
            })
            .collect::<Vec<(usize, SkillScanResult)>>()
    });

    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}

/// 扫描单个已安装 skill（用于前端展示扫描进度）
#[tauri::command]
pub async fn scan_installed_skill(
    state: State<'_, AppState>,
    app: AppHandle,
    skill_id: String,
    locale: String,
    scan_id: Option<String>,
) -> Result<SkillScanResult, String> {
    let locale = validate_locale(&locale);
    let mut skill = state
        .db
        .get_skills()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| "Skill not found".to_string())?;

    if !skill.installed || skill.local_path.is_none() {
        return Err("Skill is not installed".to_string());
    }

    let local_path = skill.local_path.clone().unwrap_or_default();
    let path = PathBuf::from(&local_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Skill directory does not exist: {}", local_path));
    }

    let scanner = SecurityScanner::new();
    let report = if let Some(scan_id) = scan_id.filter(|id| !id.is_empty()) {
        let app_handle = app.clone();
        let item_id = skill.id.clone();
        let kind = "skill".to_string();
        let mut progress_cb = |file_path: &str| {
            let payload = ScanProgressEvent {
                scan_id: scan_id.clone(),
                kind: kind.clone(),
                item_id: item_id.clone(),
                file_path: file_path.to_string(),
            };
            let _ = app_handle.emit("scan-progress", payload);
        };
        scanner
            .scan_directory_with_options(
                path.to_str().unwrap_or(""),
                &skill.id,
                &locale,
                ScanOptions { skip_readme: true },
                Some(&mut progress_cb),
            )
            .map_err(|e| e.to_string())?
    } else {
        scanner
            .scan_directory_with_options(
                path.to_str().unwrap_or(""),
                &skill.id,
                &locale,
                ScanOptions { skip_readme: true },
                None,
            )
            .map_err(|e| e.to_string())?
    };

    skill.security_score = Some(report.score);
    skill.security_level = Some(report.level.as_str().to_string());
    skill.security_issues = Some(
        report
            .issues
            .iter()
            .map(|i| {
                let file_info = i
                    .file_path
                    .as_ref()
                    .map(|f| format!("[{}] ", f))
                    .unwrap_or_default();
                format!("{}{:?}: {}", file_info, i.severity, i.description)
            })
            .collect(),
    );
    skill.scanned_at = Some(chrono::Utc::now());

    state
        .db
        .save_skill(&skill)
        .map_err(|e| format!("Failed to save skill: {}", e))?;

    Ok(SkillScanResult {
        skill_id: skill.id.clone(),
        skill_name: skill.name.clone(),
        score: report.score,
        level: report.level.as_str().to_string(),
        scanned_at: chrono::Utc::now().to_rfc3339(),
        report,
    })
}

/// 统计目录内可扫描的文件数量（用于前端进度条预估）
#[tauri::command]
pub async fn count_scan_files(
    dir_path: String,
    skip_readme: Option<bool>,
) -> Result<usize, String> {
    let path = PathBuf::from(&dir_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Directory does not exist: {}", dir_path));
    }

    let scanner = SecurityScanner::new();
    let options = ScanOptions {
        skip_readme: skip_readme.unwrap_or(true),
    };

    scanner
        .count_scan_files(path.to_str().unwrap_or(""), options)
        .map_err(|e| e.to_string())
}

/// 获取缓存的扫描结果
#[tauri::command]
pub async fn get_scan_results(state: State<'_, AppState>) -> Result<Vec<SkillScanResult>, String> {
    use crate::models::security::{IssueCategory, IssueSeverity, SecurityIssue};

    let skills = state.db.get_skills().map_err(|e| e.to_string())?;

    let results: Vec<SkillScanResult> = skills
        .into_iter()
        .filter(|s| s.installed && s.security_score.is_some())
        .map(|s| {
            // 解析 security_issues 字符串为 SecurityIssue 对象
            let issues = if let Some(issue_strings) = &s.security_issues {
                issue_strings
                    .iter()
                    .filter_map(|issue_str| {
                        // 解析格式: "[filename] Severity: description"
                        // 先提取文件路径
                        let (file_path, remaining) = if issue_str.starts_with('[') {
                            if let Some(end_bracket) = issue_str.find(']') {
                                let file = issue_str[1..end_bracket].to_string();
                                let rest = issue_str[end_bracket + 1..].trim();
                                (Some(file), rest)
                            } else {
                                (None, issue_str.as_str())
                            }
                        } else {
                            (None, issue_str.as_str())
                        };

                        // 然后解析 Severity: description
                        let parts: Vec<&str> = remaining.splitn(2, ": ").collect();
                        if parts.len() == 2 {
                            let severity = match parts[0] {
                                "Critical" => Some(IssueSeverity::Critical),
                                "Error" => Some(IssueSeverity::Error),
                                "Warning" => Some(IssueSeverity::Warning),
                                "Info" => Some(IssueSeverity::Info),
                                _ => None,
                            };

                            if let Some(severity) = severity {
                                Some(SecurityIssue {
                                    severity,
                                    category: IssueCategory::Other,
                                    description: parts[1].to_string(),
                                    line_number: None,
                                    code_snippet: None,
                                    file_path,
                                })
                            } else {
                                // 兼容旧格式（没有 Severity 前缀）：保留原始文本，避免丢失规则名等信息
                                Some(SecurityIssue {
                                    severity: IssueSeverity::Info,
                                    category: IssueCategory::Other,
                                    description: remaining.to_string(),
                                    line_number: None,
                                    code_snippet: None,
                                    file_path,
                                })
                            }
                        } else {
                            // 兼容无法解析的格式：保留原始文本
                            Some(SecurityIssue {
                                severity: IssueSeverity::Info,
                                category: IssueCategory::Other,
                                description: remaining.to_string(),
                                line_number: None,
                                code_snippet: None,
                                file_path,
                            })
                        }
                    })
                    .collect()
            } else {
                vec![]
            };

            let report = SecurityReport {
                skill_id: s.id.clone(),
                score: s.security_score.unwrap_or(0),
                level: SecurityLevel::from_score(s.security_score.unwrap_or(0)),
                issues,
                recommendations: vec![], // 建议信息暂时为空，未来可以存储到数据库
                blocked: false,
                hard_trigger_issues: vec![],
                scanned_files: vec![], // 缓存结果中没有扫描文件列表
                partial_scan: false,
                skipped_files: vec![],
            };

            SkillScanResult {
                skill_id: s.id.clone(),
                skill_name: s.name.clone(),
                score: s.security_score.unwrap_or(0),
                level: s
                    .security_level
                    .clone()
                    .unwrap_or_else(|| "Unknown".to_string()),
                scanned_at: s
                    .scanned_at
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                report,
            }
        })
        .collect();

    Ok(results)
}

/// 扫描单个 skill 文件（用于安装前检查）
///
/// # 参数
///
/// * `archive_path` - skill 文件的路径（可以是压缩包内的 SKILL.md，或已解压的文件路径）
///
/// # 返回
///
/// 返回包含安全评分、等级和问题列表的 SecurityReport
#[tauri::command]
pub async fn scan_skill_archive(
    archive_path: String,
    locale: String,
) -> Result<SecurityReport, String> {
    let locale = validate_locale(&locale);
    let scanner = SecurityScanner::new();

    // 验证文件存在性
    let path = std::path::Path::new(&archive_path);
    if !path.exists() {
        return Err(t!(
            "common.errors.file_not_found",
            locale = locale,
            path = &archive_path
        )
        .to_string());
    }
    if !path.is_file() {
        return Err(t!(
            "common.errors.path_not_file",
            locale = locale,
            path = &archive_path
        )
        .to_string());
    }

    // 读取文件内容
    let content = std::fs::read_to_string(path).map_err(|e| {
        t!(
            "common.errors.read_failed",
            locale = locale,
            path = &archive_path,
            error = e.to_string()
        )
        .to_string()
    })?;

    let report = scanner
        .scan_file(&content, &archive_path, &locale)
        .map_err(|e| {
            t!(
                "common.errors.scan_failed",
                locale = locale,
                path = &archive_path,
                error = e.to_string()
            )
            .to_string()
        })?;

    Ok(report)
}
