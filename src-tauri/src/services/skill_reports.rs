use crate::{
    models::SecurityReport,
    security::{count_kinds, cross_skill, issue_from_finding, score_from_issues},
    services::Database,
};
use anyhow::Result;
use std::collections::HashMap;

pub fn refresh_cross_skill_reports(db: &Database) -> Result<HashMap<String, SecurityReport>> {
    refresh_cross_skill_reports_with_budget(db, 64 * 1024 * 1024)
}

fn refresh_cross_skill_reports_with_budget(
    db: &Database,
    mut remaining: u64,
) -> Result<HashMap<String, SecurityReport>> {
    let skills = db.get_skills()?;
    let mut contexts = Vec::new();
    let mut incomplete = std::collections::HashSet::new();
    for skill in skills.iter().filter(|s| s.installed) {
        let Some(path) = skill.local_path.as_deref() else {
            incomplete.insert(skill.id.clone());
            continue;
        };
        let (context, partial) = cross_skill::build_scan_context_with_budget(
            skill.id.clone(),
            skill.name.clone(),
            skill.description.clone().unwrap_or_default(),
            std::path::Path::new(path),
            remaining,
        );
        if partial {
            incomplete.insert(skill.id.clone());
        }
        if let Some(context) = context {
            let bytes = context
                .file_contents
                .values()
                .map(|s| s.len() as u64)
                .sum::<u64>();
            remaining = remaining.saturating_sub(bytes);
            contexts.push(context);
        }
    }
    let findings = cross_skill::analyze_skill_set(&contexts);
    let mut reports = HashMap::new();
    for skill in &skills {
        let Some(mut report) = skill.security_report.clone() else {
            continue;
        };
        let old_count = report.issues.len();
        // A missing peer can hide a relationship for any installed skill. Preserve
        // prior hints until the whole input set has been read successfully.
        let incomplete_set = skill.installed && !incomplete.is_empty();
        report.issues.retain(|i| {
            incomplete_set
                || !i
                    .rule_id
                    .as_deref()
                    .is_some_and(|r| r.starts_with("CROSS_SKILL_"))
        });
        let had_cross = report.issues.len() != old_count;
        let relevant: Vec<_> = findings
            .iter()
            .filter(|f| skill.installed && f.skill_ids.contains(&skill.id))
            .collect();
        if incomplete_set {
            report.partial_scan = true;
            let reason =
                "Cross-skill analysis read budget exceeded or input unavailable".to_string();
            if !report.skipped_files.contains(&reason) {
                report.skipped_files.push(reason);
            }
        }
        if had_cross || !relevant.is_empty() || incomplete_set {
            for finding in relevant {
                let issue = issue_from_finding(&finding.finding);
                if !report.issues.iter().any(|old| {
                    old.rule_id == issue.rule_id
                        && old.description == issue.description
                        && old.file_path == issue.file_path
                }) {
                    report.issues.push(issue);
                }
            }
            report.score = score_from_issues(&report.issues, report.blocked);
            report.level = crate::models::SecurityLevel::from_score(report.score);
            report.kind_counts = Some(count_kinds(&report.issues));
            if !db.update_skill_associations_if_current(skill, &report)? {
                continue;
            }
        }
        reports.insert(skill.id.clone(), report);
    }
    Ok(reports)
}

#[cfg(test)]
mod review_followups {
    use super::*;
    use crate::{models::Skill, security::scan_directory};

    fn fixture() -> (tempfile::TempDir, Database, Vec<Skill>) {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::new(temp.path().join("db")).unwrap();
        let mut skills = Vec::new();
        for name in ["a", "b"] {
            let path = temp.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("SKILL.md"), "Format documents.").unwrap();
            std::fs::write(
                path.join("notes.txt"),
                "https://shared.example.invalid/path",
            )
            .unwrap();
            let mut skill = Skill::new(name.into(), "local".into(), name.into());
            skill.installed = true;
            skill.local_path = Some(path.to_string_lossy().into());
            skill.security_report = Some(
                scan_directory(path.to_str().unwrap(), &skill.id, "en", Default::default())
                    .unwrap(),
            );
            db.save_skill(&skill).unwrap();
            skills.push(skill);
        }
        (temp, db, skills)
    }

    fn cross_count(report: &SecurityReport) -> usize {
        report
            .issues
            .iter()
            .filter(|i| {
                i.rule_id
                    .as_deref()
                    .is_some_and(|r| r.starts_with("CROSS_SKILL_"))
            })
            .count()
    }

    #[test]
    fn associations_preserve_full_scan_timestamp_including_none() {
        let (_temp, db, mut skills) = fixture();
        skills[0].scanned_at = Some(
            chrono::DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );
        db.save_skill(&skills[0]).unwrap();
        refresh_cross_skill_reports(&db).unwrap();
        for saved in db.get_skills().unwrap() {
            let old = skills.iter().find(|s| s.id == saved.id).unwrap();
            assert_eq!(saved.scanned_at, old.scanned_at);
            assert!(cross_count(saved.security_report.as_ref().unwrap()) > 0);
        }
        let report = skills[0].security_report.as_ref().unwrap();
        assert!(db.update_skill_scan_if_current(&skills[0], report).unwrap());
        assert!(
            db.get_skills()
                .unwrap()
                .iter()
                .find(|s| s.id == skills[0].id)
                .unwrap()
                .scanned_at
                > skills[0].scanned_at
        );
    }

    #[test]
    fn incomplete_peer_preserves_both_hints_until_complete_analysis() {
        let (_temp, db, skills) = fixture();
        let original = refresh_cross_skill_reports(&db).unwrap();
        for budget in [0, 40] {
            let reports = refresh_cross_skill_reports_with_budget(&db, budget).unwrap();
            for skill in &skills {
                assert_eq!(
                    cross_count(&reports[&skill.id]),
                    cross_count(&original[&skill.id])
                );
                assert!(reports[&skill.id].partial_scan);
            }
        }
        let peer = std::path::Path::new(skills[1].local_path.as_ref().unwrap());
        std::fs::write(peer.join("notes.txt"), "x".repeat(3 * 1024 * 1024)).unwrap();
        let reports = refresh_cross_skill_reports(&db).unwrap();
        for skill in &skills {
            assert_eq!(
                cross_count(&reports[&skill.id]),
                cross_count(&original[&skill.id])
            );
            assert!(reports[&skill.id].partial_scan);
        }
        std::fs::remove_file(peer.join("notes.txt")).unwrap();
        let reports = refresh_cross_skill_reports(&db).unwrap();
        assert!(reports.values().all(|r| cross_count(r) == 0));
    }
}
