//! security rule matrix integration tests (manifest-driven fixtures)

use agent_skills_guard_lib::security::policy::SeverityOverride;
use agent_skills_guard_lib::security::{ScanOptions, ScanPolicy, SecurityScanner};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/security/rule_matrix")
}

fn rule_id_set(
    report: &agent_skills_guard_lib::models::security::SecurityReport,
) -> HashSet<String> {
    report
        .issues
        .iter()
        .filter_map(|i| i.rule_id.clone())
        .collect()
}

#[derive(Debug, Deserialize)]
struct RuleMatrixManifest {
    version: u32,
    cases: Vec<RuleMatrixCase>,
}

#[derive(Debug, Deserialize)]
struct RuleMatrixCase {
    id: String,
    path: String,
    #[serde(default)]
    expect_any: Vec<String>,
    #[serde(default)]
    expect_none: Vec<String>,
    /// 期望扫描结果被 hard_trigger 阻止（可选）
    #[serde(default)]
    expect_blocked: Option<bool>,
    /// 额外禁用的规则 ID（合并进扫描策略）
    #[serde(default)]
    disabled_rules: Vec<String>,
}

fn scan_options_for_case(case: &RuleMatrixCase) -> ScanOptions {
    let mut policy = ScanPolicy::builtin_default().clone();
    for rule_id in &case.disabled_rules {
        policy.disabled_rules.insert(rule_id.clone());
    }
    ScanOptions::with_policy(policy)
}

fn load_manifest() -> RuleMatrixManifest {
    let path = fixture_root().join("manifest.yaml");
    let yaml = std::fs::read_to_string(&path).expect("read manifest.yaml");
    serde_yaml::from_str(&yaml).expect("parse manifest.yaml")
}

fn run_case(case: &RuleMatrixCase) {
    let dir = fixture_root().join(&case.path);
    assert!(
        dir.is_dir(),
        "fixture dir missing for {}: {:?}",
        case.id,
        dir
    );

    let scanner = SecurityScanner::new();
    let report = scanner
        .scan_directory_with_options(
            dir.to_str().unwrap(),
            &format!("rule-matrix-{}", case.id),
            "en",
            scan_options_for_case(case),
            None,
        )
        .unwrap_or_else(|e| panic!("scan failed for {}: {}", case.id, e));

    let ids = rule_id_set(&report);

    for rule_id in &case.expect_any {
        assert!(
            ids.contains(rule_id),
            "case {}: expected rule {}, got {:?}",
            case.id,
            rule_id,
            ids
        );
    }

    for rule_id in &case.expect_none {
        assert!(
            !ids.contains(rule_id),
            "case {}: rule {} should be absent, got {:?}",
            case.id,
            rule_id,
            ids
        );
    }

    if let Some(expect_blocked) = case.expect_blocked {
        assert_eq!(
            report.blocked, expect_blocked,
            "case {}: expected blocked={}, got blocked={}",
            case.id, expect_blocked, report.blocked
        );
    }

    if case.expect_any.is_empty()
        && case.expect_none.is_empty()
        && case.expect_blocked.is_none()
    {
        panic!("case {} has no expectations", case.id);
    }
}

#[test]
fn rule_matrix_manifest_all_cases() {
    let manifest = load_manifest();
    assert_eq!(manifest.version, 1);
    assert!(!manifest.cases.is_empty(), "manifest must list cases");
    for case in &manifest.cases {
        run_case(case);
    }
}

#[test]
fn rule_matrix_manifest_case_count() {
    let manifest = load_manifest();
    assert!(
        manifest.cases.len() >= 55,
        "expected at least 55 rule matrix cases, got {}",
        manifest.cases.len()
    );
}

#[test]
fn rule_matrix_report_metadata_has_policy_fingerprint() {
    let case = load_manifest()
        .cases
        .into_iter()
        .find(|c| c.id == "path_traversal_open")
        .expect("path_traversal_open case in manifest");
    let dir = fixture_root().join(&case.path);
    let scanner = SecurityScanner::new();
    let report = scanner
        .scan_directory_with_options(
            dir.to_str().unwrap(),
            "meta",
            "en",
            ScanOptions::default(),
            None,
        )
        .unwrap();
    let meta = report.metadata.expect("metadata should be set");
    assert!(meta.policy_fingerprint.is_some());
    assert_eq!(meta.policy_name.as_deref(), Some("default"));
}

#[test]
fn rule_matrix_severity_override_demotes_issue() {
    let mut policy = ScanPolicy::builtin_default().clone();
    policy.severity_overrides.push(SeverityOverride {
        rule_id: "CURL_PIPE_SH".to_string(),
        severity: "Info".to_string(),
        reason: "test: demote for rule matrix".to_string(),
    });
    let dir = fixture_root().join("p0-signatures/curl-pipe");
    let scanner = SecurityScanner::new();
    let report = scanner
        .scan_directory_with_options(
            dir.to_str().unwrap(),
            "severity-override-test",
            "en",
            ScanOptions::with_policy(policy),
            None,
        )
        .expect("scan should succeed");

    let curl_issue = report
        .issues
        .iter()
        .find(|i| i.rule_id.as_deref() == Some("CURL_PIPE_SH"));
    let issue = curl_issue.expect("CURL_PIPE_SH should still be present");
    assert_eq!(
        issue.severity,
        agent_skills_guard_lib::models::security::IssueSeverity::Info,
        "severity should be demoted to Info by override"
    );
}
