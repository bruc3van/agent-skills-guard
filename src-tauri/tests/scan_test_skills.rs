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

/// 识别 skill 目录：如果子目录中包含 SKILL.md，则视为一个 skill
/// 对于 security-test-hskills 这种包含嵌套 skill 的父目录，展开为其子 skill
fn collect_skill_dirs(root: &PathBuf) -> Vec<(String, PathBuf)> {
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();

    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_string();

        // 如果该目录本身包含 SKILL.md，它就是一个 skill
        if path.join("SKILL.md").exists() {
            dirs.push((name, path));
        } else {
            // 否则检查子目录是否包含 SKILL.md（嵌套 skill 集合）
            for child in fs::read_dir(&path).unwrap() {
                let child = child.unwrap();
                let child_path = child.path();
                if child_path.is_dir() && child_path.join("SKILL.md").exists() {
                    let child_name = format!(
                        "{}/{}",
                        name,
                        child_path.file_name().unwrap().to_str().unwrap()
                    );
                    dirs.push((child_name, child_path));
                }
            }
        }
    }

    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    dirs
}

#[test]
fn scan_all_test_skills() {
    let root = test_skills_root();
    assert!(root.is_dir(), "test-skills dir not found: {:?}", root);

    let scanner = SecurityScanner::new();
    let skill_dirs = collect_skill_dirs(&root);

    println!(
        "\n=== Scanning {} skill directories ===\n",
        skill_dirs.len()
    );

    let mut reports = Vec::new();

    for (name, dir) in &skill_dirs {
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
