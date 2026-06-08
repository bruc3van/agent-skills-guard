//! Scan test-skills directory and output a JSON report

use agent_skills_guard_lib::security::{ScanOptions, SecurityScanner};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

fn test_skills_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test")
        .join("test-skills")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureKind {
    PositiveReal,
    NegativeGenerated,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureExpectation {
    #[serde(default)]
    blocked: Option<bool>,
    #[serde(default)]
    required_rule_ids: Vec<String>,
    #[serde(default)]
    forbidden_rule_ids: Vec<String>,
}

fn fixture_kind(name: &str) -> Option<FixtureKind> {
    if name.starts_with("positive-real/") {
        Some(FixtureKind::PositiveReal)
    } else if name.starts_with("negative-generated/") {
        Some(FixtureKind::NegativeGenerated)
    } else {
        None
    }
}

fn read_expectation(dir: &PathBuf) -> FixtureExpectation {
    let path = dir.join("expected.json");
    if !path.exists() {
        return FixtureExpectation::default();
    }

    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse {}: {}", path.display(), e))
}

/// Recursively collect fixture skill directories. A directory is a skill when it
/// contains SKILL.md; parent fixture groups are only containers.
fn collect_skill_dirs(root: &PathBuf) -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    collect_skill_dirs_inner(root, root, &mut dirs);
    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    dirs
}

fn collect_skill_dirs_inner(root: &PathBuf, current: &PathBuf, dirs: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(current).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if path.join("SKILL.md").exists() {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            dirs.push((name, path));
        } else {
            collect_skill_dirs_inner(root, &path, dirs);
        }
    }
}

#[test]
fn scan_all_test_skills() {
    let root = test_skills_root();
    assert!(root.is_dir(), "test-skills dir not found: {:?}", root);

    let scanner = SecurityScanner::new();
    let skill_dirs = collect_skill_dirs(&root);
    let positive_count = skill_dirs
        .iter()
        .filter(|(name, _)| fixture_kind(name) == Some(FixtureKind::PositiveReal))
        .count();
    let negative_count = skill_dirs
        .iter()
        .filter(|(name, _)| fixture_kind(name) == Some(FixtureKind::NegativeGenerated))
        .count();

    assert!(
        positive_count > 0,
        "expected real installed skill fixtures under positive-real/"
    );
    assert!(
        negative_count > 0,
        "expected generated malicious fixtures under negative-generated/"
    );

    println!(
        "\n=== Scanning {} skill directories ===\n",
        skill_dirs.len()
    );

    let mut reports = Vec::new();
    let mut failures = Vec::new();

    for (name, dir) in &skill_dirs {
        let kind = fixture_kind(name)
            .unwrap_or_else(|| panic!("unexpected fixture outside known groups: {}", name));
        let expectation = read_expectation(dir);
        let dir_str = dir.to_str().unwrap();
        println!("Scanning: {} ...", name);

        match scanner.scan_directory_with_options(
            dir_str,
            &format!("test-{}", name.replace('/', "-").replace('\\', "-")),
            "en",
            ScanOptions::default(),
            None,
        ) {
            Ok(report) => {
                let level = &report.level;
                let score = report.score;
                let blocked = report.blocked;
                let issue_count = report.issues.len();
                let scanned = report.scanned_files.len();
                let skipped = report.skipped_files.len();

                println!(
                    "  Score: {} | Level: {:?} | Blocked: {} | Issues: {} | Scanned: {} | Skipped: {}",
                    score, level, blocked, issue_count, scanned, skipped
                );

                if !report.issues.is_empty() {
                    println!("  Issues:");
                    for issue in &report.issues {
                        let severity = &issue.severity;
                        let rule = issue.rule_id.as_deref().unwrap_or("N/A");
                        let file = issue.file_path.as_deref().unwrap_or("unknown");
                        let line = issue
                            .line_number
                            .map(|l| l.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        let desc = &issue.description;
                        println!(
                            "    [{:?}] {} | {}:{} | {}",
                            severity, rule, file, line, desc,
                        );
                    }
                }

                if !report.recommendations.is_empty() {
                    println!("  Recommendations:");
                    for rec in &report.recommendations {
                        println!("    - {}", rec);
                    }
                }

                if !report.hard_trigger_issues.is_empty() {
                    println!("  Hard Triggers: {:?}", report.hard_trigger_issues);
                }

                match kind {
                    FixtureKind::PositiveReal => {
                        let expected_blocked = expectation.blocked.unwrap_or(false);
                        if report.blocked != expected_blocked || !report.hard_trigger_issues.is_empty() {
                            failures.push(format!(
                                "real skill fixture '{}' should have blocked={}; blocked={}, hard_triggers={:?}",
                                name, expected_blocked, report.blocked, report.hard_trigger_issues
                            ));
                        }
                    }
                    FixtureKind::NegativeGenerated => {
                        if let Some(expected_blocked) = expectation.blocked {
                            if report.blocked != expected_blocked {
                                failures.push(format!(
                                    "negative skill fixture '{}' should have blocked={}; blocked={}, hard_triggers={:?}",
                                    name, expected_blocked, report.blocked, report.hard_trigger_issues
                                ));
                            }
                        }
                        if expectation.blocked == Some(true) && report.hard_trigger_issues.is_empty()
                        {
                            failures.push(format!(
                                "negative skill fixture '{}' should include hard-trigger details; blocked={}, hard_triggers={:?}, issues={:?}",
                                name, report.blocked, report.hard_trigger_issues, report.issues
                            ));
                        }
                    }
                }

                for required in &expectation.required_rule_ids {
                    if !report
                        .issues
                        .iter()
                        .any(|issue| issue.rule_id.as_deref() == Some(required.as_str()))
                    {
                        failures.push(format!(
                            "fixture '{}' missing required rule_id '{}'; issues={:?}",
                            name, required, report.issues
                        ));
                    }
                }

                for forbidden in &expectation.forbidden_rule_ids {
                    if report
                        .issues
                        .iter()
                        .any(|issue| issue.rule_id.as_deref() == Some(forbidden.as_str()))
                    {
                        failures.push(format!(
                            "fixture '{}' unexpectedly reported forbidden rule_id '{}'; issues={:?}",
                            name, forbidden, report.issues
                        ));
                    }
                }

                reports.push(serde_json::json!({
                    "skill_id": name,
                    "score": score,
                    "level": format!("{:?}", level),
                    "blocked": blocked,
                    "issue_count": issue_count,
                    "scanned_files": scanned,
                    "skipped_files": skipped,
                    "issues": report.issues.iter().map(|i| serde_json::json!({
                        "severity": format!("{:?}", i.severity),
                        "rule_id": i.rule_id,
                        "file_path": i.file_path,
                        "line_number": i.line_number,
                        "description": i.description,
                        "category": i.category,
                        "cwe_id": i.cwe_id,
                        "confidence": i.confidence,
                    })).collect::<Vec<_>>(),
                    "recommendations": report.recommendations,
                    "hard_trigger_issues": report.hard_trigger_issues,
                }));

                println!();
            }
            Err(e) => {
                println!("  ERROR: {}\n", e);
                failures.push(format!("{} failed to scan: {}", name, e));
                reports.push(serde_json::json!({
                    "skill_id": name,
                    "error": format!("{}", e),
                }));
            }
        }
    }

    // Output summary JSON
    println!("\n=== SUMMARY JSON ===");
    println!("{}", serde_json::to_string_pretty(&reports).unwrap());

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
