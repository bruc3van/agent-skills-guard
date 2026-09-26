use crate::commands::{clamp_scan_parallelism, scan_thread_pool, AppState};
use crate::i18n::validate_locale;
use crate::models::security::{SecurityLevel, SecurityReport, SkillScanResult};
use crate::models::Skill;
use crate::security::{scan_directory, ScanOptions};
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
    if !db.update_skill_scan_if_current(skill, report)? {
        anyhow::bail!("SCAN_RESULT_STALE: installation changed during scanning");
    }
    Ok(())
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

    let parallelism = clamp_scan_parallelism(scan_parallelism);
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || {
        let manager = skill_manager.blocking_lock();
        let installed = manager
            .get_installed_skills()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.installed && s.local_path.is_some())
            .collect();
        scan_installed_skills_blocking(db, installed, locale, parallelism)
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
                    return None;
                }

                let identity = match crate::services::content_manifest::manifest(&path) {
                    Ok(identity) => identity,
                    Err(error) => {
                        log::warn!("Cannot establish scan identity: {error}");
                        return None;
                    }
                };
                let report = match scan_directory(
                    path.to_str().unwrap_or(""),
                    &skill.id,
                    &locale_owned,
                    ScanOptions {
                        skip_readme: false,
                        ..Default::default()
                    },
                ) {
                    Ok(report) => report,
                    Err(e) => {
                        log::warn!("Failed to scan skill {}: {}", skill.name, e);
                        return None;
                    }
                };

                // 出站前剥离 scanned_files（前端不读，仅徒增 DB / IPC 体积）
                let report = report.without_scanned_file_list();

                if crate::services::content_manifest::manifest(&path)
                    .ok()
                    .as_ref()
                    != Some(&identity)
                {
                    log::warn!("Discarding scan of changed content: {}", skill.name);
                    return None;
                }
                if let Err(e) = persist_skill_scan_report(&db, skill, &report) {
                    log::warn!("Failed to save skill {}: {}", skill.name, e);
                    return None;
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

    let reports = crate::services::skill_reports::refresh_cross_skill_reports(&db)
        .map_err(|e| e.to_string())?;
    for (_, result) in &mut results {
        if let Some(report) = reports.get(&result.skill_id) {
            result.report = report.clone();
            result.score = report.score;
            result.level = report.level.as_str().to_string();
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

    let manager = Arc::clone(&state.skill_manager);
    tokio::task::spawn_blocking(move || {
        let _guard = manager.blocking_lock();
        scan_installed_skill_blocking(db, &skill_id, &locale)
    })
    .await
    .map_err(|e| format!("[TASK_JOIN_ERROR] {e}"))?
}

/// `scan_installed_skill` 的同步主体，必须在阻塞线程上运行。
fn scan_installed_skill_blocking(
    db: Arc<crate::services::Database>,
    skill_id: &str,
    locale: &str,
) -> Result<SkillScanResult, String> {
    let skill = db
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
        return Err(format!(
            "[SKILL_DIR_NOT_FOUND] Skill directory does not exist: {}",
            local_path
        ));
    }

    let identity = crate::services::content_manifest::manifest(&path).map_err(|e| e.to_string())?;
    let report = scan_directory(
        path.to_str().unwrap_or(""),
        &skill.id,
        locale,
        ScanOptions {
            skip_readme: false,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?
    // 出站前剥离 scanned_files（前端不读，仅徒增 DB / IPC 体积）
    .without_scanned_file_list();

    if crate::services::content_manifest::manifest(&path)
        .ok()
        .as_ref()
        != Some(&identity)
    {
        return Err("SCAN_RESULT_STALE: content changed during scanning".into());
    }
    persist_skill_scan_report(&db, &skill, &report).map_err(|e| e.to_string())?;

    let reports = crate::services::skill_reports::refresh_cross_skill_reports(&db)
        .map_err(|e| e.to_string())?;
    let report = reports.get(&skill.id).cloned().unwrap_or(report);

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

        let options = ScanOptions {
            skip_readme: skip_readme.unwrap_or(true),
            ..Default::default()
        };

        crate::security::count_scan_files(path.to_str().unwrap_or(""), options)
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
