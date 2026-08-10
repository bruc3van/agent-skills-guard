use crate::commands::{clamp_scan_parallelism, scan_thread_pool, AppState};
use crate::i18n::validate_locale;
use crate::models::security::{SecurityLevel, SecurityReport, SkillScanResult};
use crate::models::Skill;
use crate::security::cross_skill::{self, SkillScanContext};
use crate::security::{ScanOptions, SecurityScanner};
use anyhow::Result;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

fn persist_skill_scan_report(
    db: &crate::services::Database,
    skill: &Skill,
    report: &SecurityReport,
) -> Result<()> {
    let mut updated = skill.clone();
    updated.security_score = Some(report.score);
    updated.security_level = Some(report.level.as_str().to_string());
    updated.security_issues = Some(report.issues.clone());
    updated.security_report = Some(report.clone());
    updated.scanned_at = Some(chrono::Utc::now());
    db.save_skill(&updated)
}

/// 扫描所有已安装的 skills
#[tauri::command]
pub async fn scan_all_installed_skills(
    state: State<'_, AppState>,
    locale: String,
    scan_parallelism: Option<usize>,
) -> Result<Vec<SkillScanResult>, String> {
    let locale = validate_locale(&locale).to_string();
    let skill_manager = Arc::clone(&state.skill_manager);

    // 读取已安装列表本身也是重活（会遍历各工具目录、比对软链），
    // 同样不能占用 async 工作线程。
    let skills = tokio::task::spawn_blocking(move || {
        let manager = skill_manager.blocking_lock();
        manager.get_installed_skills().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))??;

    let installed_skills: Vec<Skill> = skills
        .into_iter()
        .filter(|s| s.installed && s.local_path.is_some())
        .collect();

    let parallelism = clamp_scan_parallelism(scan_parallelism);
    let db = state.db.clone();

    // 扫描是纯 CPU + 磁盘 IO 的同步任务。此前直接在 async 命令里
    // `pool.install(...)`，会阻塞 tokio 工作线程直到全部扫完，期间其他 IPC
    // 请求排队 —— 这正是扫描期间整个界面卡顿的原因。
    tokio::task::spawn_blocking(move || {
        scan_installed_skills_blocking(db, installed_skills, locale, parallelism)
    })
    .await
    .map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))?
}

/// `scan_all_installed_skills` 的同步主体，必须在阻塞线程上运行。
fn scan_installed_skills_blocking(
    db: Arc<crate::services::Database>,
    installed_skills: Vec<Skill>,
    locale: String,
    parallelism: usize,
) -> Result<Vec<SkillScanResult>, String> {
    let locale_owned = locale;
    let pool = scan_thread_pool(parallelism)?;

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
                    log::warn!(
                        "Skill directory does not exist, marking as uninstalled: {:?}",
                        path
                    );
                    let mut updated = skill.clone();
                    updated.installed = false;
                    updated.installed_at = None;
                    updated.local_path = None;
                    updated.local_paths = None;
                    updated.source_path = None;
                    updated.linked_tools = Vec::new();
                    if let Err(e) = db.save_skill(&updated) {
                        log::warn!("Failed to update stale skill '{}': {}", skill.name, e);
                    }
                    return None;
                }

                let scanner = SecurityScanner::new();
                let report = match scanner.scan_directory_with_options(
                    path.to_str().unwrap_or(""),
                    &skill.id,
                    &locale_owned,
                    ScanOptions {
                        skip_readme: true,
                        ..Default::default()
                    },
                    None,
                ) {
                    Ok(report) => report,
                    Err(e) => {
                        log::warn!("Failed to scan skill {}: {}", skill.name, e);
                        return None;
                    }
                };

                // 出站前剥离 scanned_files（前端不读，仅徒增 DB / IPC 体积）
                let report = report.without_scanned_file_list();

                if let Err(e) = persist_skill_scan_report(&db, skill, &report) {
                    log::warn!("Failed to save skill {}: {}", skill.name, e);
                }

                Some((
                    index,
                    SkillScanResult {
                        skill_id: skill.id.clone(),
                        skill_name: skill.name.clone(),
                        score: report.score,
                        level: report.level.as_str().to_string(),
                        scanned_at: chrono::Utc::now().to_rfc3339(),
                        report,
                    },
                ))
            })
            .collect::<Vec<(usize, SkillScanResult)>>()
    });

    // ── 跨 Skill 协同攻击检测 ──
    let cross_skill_contexts: Vec<SkillScanContext> = installed_skills
        .iter()
        .filter_map(|skill| {
            let local_path = skill.local_path.as_ref()?;
            let path = PathBuf::from(local_path);
            if !path.exists() || !path.is_dir() {
                return None;
            }
            cross_skill::build_scan_context_from_skill_dir(
                skill.id.clone(),
                skill.name.clone(),
                skill.description.clone().unwrap_or_default(),
                &path,
            )
        })
        .collect();

    if cross_skill_contexts.len() >= 2 {
        let cross_findings = cross_skill::analyze_skill_set(&cross_skill_contexts);
        if !cross_findings.is_empty() {
            // 将 cross-skill findings 追加到所有已扫描 skill 的 report 中
            for (_, result) in &mut results {
                let cross_issues: Vec<_> = cross_findings
                    .iter()
                    .map(|f| crate::models::security::SecurityIssue {
                        severity: f.severity,
                        category: f.category.to_issue_category(),
                        description: f.description.clone(),
                        line_number: None,
                        code_snippet: None,
                        file_path: None,
                        rule_id: Some(f.rule_id.clone()),
                        confidence: f.metadata.as_ref().and_then(|m| m.confidence.clone()),
                        remediation: f.remediation.clone(),
                        cwe_id: f.metadata.as_ref().and_then(|m| m.cwe_id.clone()),
                        threat_category: Some(f.category.as_str().to_string()),
                        same_path_other_rule_ids: None,
                        finding_kind: f
                            .metadata
                            .as_ref()
                            .and_then(|m| m.finding_kind)
                            .map(|k| k.as_str().to_string()),
                    })
                    .collect();
                result.report.issues.extend(cross_issues);
                let new_score = SecurityScanner::score_from_issues(
                    &result.report.issues,
                    result.report.blocked,
                );
                result.score = new_score;
                result.report.score = new_score;
                result.level = SecurityLevel::from_score(new_score).as_str().to_string();
                result.report.level = SecurityLevel::from_score(new_score);

                // 单体扫描结果在 cross-skill 分析前已经落库；追加协同攻击 finding
                // 后必须再次保存，否则 IPC 当次显示的分数会在重启/切页后消失。
                if let Some(original) = installed_skills
                    .iter()
                    .find(|skill| skill.id == result.skill_id)
                {
                    if let Err(error) = persist_skill_scan_report(&db, original, &result.report) {
                        log::warn!(
                            "Failed to persist cross-skill findings for '{}': {}",
                            original.name,
                            error
                        );
                    }
                }
            }
        }
    }

    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
}

/// 扫描单个已安装 skill
#[tauri::command]
pub async fn scan_installed_skill(
    state: State<'_, AppState>,
    skill_id: String,
    locale: String,
) -> Result<SkillScanResult, String> {
    let locale = validate_locale(&locale).to_string();
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || scan_installed_skill_blocking(db, &skill_id, &locale))
        .await
        .map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))?
}

/// `scan_installed_skill` 的同步主体，必须在阻塞线程上运行。
fn scan_installed_skill_blocking(
    db: Arc<crate::services::Database>,
    skill_id: &str,
    locale: &str,
) -> Result<SkillScanResult, String> {
    let mut skill = db
        .get_skills()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.id == skill_id)
        .ok_or_else(|| "[SKILL_NOT_FOUND] Skill not found".to_string())?;

    if !skill.installed || skill.local_path.is_none() {
        return Err("[SKILL_NOT_INSTALLED] Skill is not installed".to_string());
    }

    let local_path = skill.local_path.clone().unwrap_or_default();
    let path = PathBuf::from(&local_path);
    if !path.exists() || !path.is_dir() {
        // Update DB: mark as uninstalled since directory is gone
        skill.installed = false;
        skill.installed_at = None;
        skill.local_path = None;
        skill.local_paths = None;
        skill.source_path = None;
        skill.linked_tools = Vec::new();
        if let Err(e) = db.save_skill(&skill) {
            log::warn!("Failed to update stale skill '{}': {}", skill.name, e);
        }
        return Err(format!(
            "[SKILL_DIR_NOT_FOUND] Skill directory does not exist: {}",
            local_path
        ));
    }

    let scanner = SecurityScanner::new();
    let report = scanner
        .scan_directory_with_options(
            path.to_str().unwrap_or(""),
            &skill.id,
            locale,
            ScanOptions {
                skip_readme: true,
                ..Default::default()
            },
            None,
        )
        .map_err(|e| e.to_string())?
        // 出站前剥离 scanned_files（前端不读，仅徒增 DB / IPC 体积）
        .without_scanned_file_list();

    skill.security_score = Some(report.score);
    skill.security_level = Some(report.level.as_str().to_string());
    skill.security_issues = Some(report.issues.clone());
    skill.security_report = Some(report.clone());
    skill.scanned_at = Some(chrono::Utc::now());

    db.save_skill(&skill)
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
    tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&dir_path);
        if !path.exists() || !path.is_dir() {
            return Err(format!(
                "[DIR_NOT_FOUND] Directory does not exist: {}",
                dir_path
            ));
        }

        let scanner = SecurityScanner::new();
        let options = ScanOptions {
            skip_readme: skip_readme.unwrap_or(true),
            ..Default::default()
        };

        scanner
            .count_scan_files(path.to_str().unwrap_or(""), options)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))?
}

/// 获取缓存的扫描结果
#[tauri::command]
pub async fn get_scan_results(state: State<'_, AppState>) -> Result<Vec<SkillScanResult>, String> {
    let db = state.db.clone();

    // 全表读 + 每条记录的 security_report JSON 反序列化，量大时不应占用
    // async 工作线程
    tokio::task::spawn_blocking(move || collect_scan_results(&db))
        .await
        .map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))?
}

fn collect_scan_results(db: &crate::services::Database) -> Result<Vec<SkillScanResult>, String> {
    let skills = db.get_skills().map_err(|e| e.to_string())?;

    let results: Vec<SkillScanResult> = skills
        .into_iter()
        .filter(|s| s.installed && s.security_score.is_some())
        .map(|s| {
            let report = s.security_report.clone().unwrap_or_else(|| SecurityReport {
                skill_id: s.id.clone(),
                score: s.security_score.unwrap_or(0),
                level: s
                    .security_level
                    .as_deref()
                    .and_then(|level| level.parse().ok())
                    .unwrap_or_else(|| SecurityLevel::from_score(s.security_score.unwrap_or(0))),
                issues: s.security_issues.clone().unwrap_or_default(),
                recommendations: vec![],
                blocked: false,
                hard_trigger_issues: vec![],
                scanned_files: vec![],
                partial_scan: false,
                skipped_files: vec![],
                metadata: None,
                kind_counts: None,
            });

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::security::{IssueSeverity, SecurityIssue};

    #[test]
    fn persisted_report_keeps_post_scan_cross_skill_findings() {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::services::Database::new(temp.path().join("test.db")).unwrap();
        let mut skill = Skill::new(
            "first".to_string(),
            "https://github.com/example/repo".to_string(),
            "skills/first".to_string(),
        );
        skill.installed = true;
        db.save_skill(&skill).unwrap();

        let report = SecurityReport {
            skill_id: skill.id.clone(),
            score: 29,
            level: SecurityLevel::Critical,
            issues: vec![SecurityIssue {
                severity: IssueSeverity::Critical,
                rule_id: Some("CROSS_SKILL_PAYLOAD_CHAIN".to_string()),
                description: "cross-skill chain".to_string(),
                ..Default::default()
            }],
            recommendations: Vec::new(),
            blocked: true,
            hard_trigger_issues: vec!["CROSS_SKILL_PAYLOAD_CHAIN".to_string()],
            scanned_files: Vec::new(),
            partial_scan: false,
            skipped_files: Vec::new(),
            metadata: None,
            kind_counts: None,
        };

        persist_skill_scan_report(&db, &skill, &report).unwrap();

        let saved = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == skill.id)
            .unwrap();
        assert_eq!(saved.security_score, Some(29));
        assert!(saved
            .security_report
            .unwrap()
            .issues
            .iter()
            .any(|issue| { issue.rule_id.as_deref() == Some("CROSS_SKILL_PAYLOAD_CHAIN") }));
    }
}
