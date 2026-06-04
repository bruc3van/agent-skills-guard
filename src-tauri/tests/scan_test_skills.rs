//! Scan test-skills directory and output a JSON report

use agent_skills_guard_lib::security::{ScanOptions, SecurityScanner};
use std::fs;
use std::path::PathBuf;

fn test_skills_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("test")
        .join("test-skills")
}

#[test]
fn scan_all_test_skills() {
    let root = test_skills_root();
    assert!(root.is_dir(), "test-skills dir not found: {:?}", root);

    let scanner = SecurityScanner::new();

    // Collect all immediate subdirectories as skill dirs
    let mut skill_dirs: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            skill_dirs.push((name, path));
        }
    }
    skill_dirs.sort_by(|a, b| a.0.cmp(&b.0));

    println!("\n=== Scanning {} skill directories ===\n", skill_dirs.len());

    let mut reports = Vec::new();

    for (name, dir) in &skill_dirs {
        let dir_str = dir.to_str().unwrap();
        println!("Scanning: {} ...", name);

        match scanner.scan_directory_with_options(
            dir_str,
            &format!("test-{}", name),
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
                        let line = issue.line_number.map(|l| l.to_string()).unwrap_or_else(|| "-".to_string());
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
}
