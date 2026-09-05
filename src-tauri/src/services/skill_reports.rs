use crate::{
    models::SecurityReport,
    security::{cross_skill, SecurityScanner},
    services::Database,
};
use anyhow::Result;
use std::collections::HashMap;

pub fn refresh_cross_skill_reports(db: &Database) -> Result<HashMap<String, SecurityReport>> {
    let skills = db.get_skills()?;
    let contexts: Vec<_> = skills
        .iter()
        .filter(|s| s.installed)
        .filter_map(|s| {
            cross_skill::build_scan_context_from_skill_dir(
                s.id.clone(),
                s.name.clone(),
                s.description.clone().unwrap_or_default(),
                std::path::Path::new(s.local_path.as_ref()?),
            )
        })
        .collect();
    let findings = cross_skill::analyze_skill_set(&contexts);
    let mut reports = HashMap::new();
    for skill in &skills {
        let Some(mut report) = skill.security_report.clone() else {
            continue;
        };
        let old_count = report.issues.len();
        report.issues.retain(|i| {
            !i.rule_id
                .as_deref()
                .is_some_and(|r| r.starts_with("CROSS_SKILL_"))
        });
        let had_cross = report.issues.len() != old_count;
        let relevant: Vec<_> = findings
            .iter()
            .filter(|f| skill.installed && f.skill_ids.contains(&skill.id))
            .collect();
        if had_cross || !relevant.is_empty() {
            report.issues.extend(
                relevant
                    .iter()
                    .map(|f| SecurityScanner::issue_from_finding(&f.finding)),
            );
            report.score = SecurityScanner::score_from_issues(&report.issues, report.blocked);
            report.level = crate::models::SecurityLevel::from_score(report.score);
            report.kind_counts = Some(SecurityScanner::count_kinds(&report.issues));
            db.update_skill_security_report(&skill.id, &report)?;
        }
        reports.insert(skill.id.clone(), report);
    }
    Ok(reports)
}
