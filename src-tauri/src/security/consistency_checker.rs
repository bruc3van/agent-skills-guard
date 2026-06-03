//! 一致性检查模块（Consistency Checker）
//!
//! 检查 Skill 的 allowed-tools 声明与实际代码行为的一致性、
//! manifest 行为一致性、以及 description 质量。
//!
//! - `check_allowed_tools`: allowed-tools 声明 vs 代码实际使用
//! - `check_manifest_consistency`: manifest 元数据与代码行为一致性
//! - `check_description_quality`: description 泛化/质量检查

use lazy_static::lazy_static;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use crate::security::skill_context::{SkillContext, SkillFileType};

// ── 常量 ──

const ANALYZER_NAME: &str = "consistency_checker";

// ── 正则表达式 ──

lazy_static! {
    // Read 能力模式
    static ref RE_READ: Vec<Regex> = vec![
        Regex::new(r#"open\(.*['"]r"#).unwrap(),
        Regex::new(r"\.read\(\)").unwrap(),
        Regex::new(r"\.read_text\(").unwrap(),
        Regex::new(r"\.read_bytes\(").unwrap(),
    ];

    // Write 能力模式
    static ref RE_WRITE: Vec<Regex> = vec![
        Regex::new(r#"open\(.*['"]w"#).unwrap(),
        Regex::new(r"\.write\(").unwrap(),
        Regex::new(r"\.writelines\(").unwrap(),
        Regex::new(r"\.write_text\(").unwrap(),
        Regex::new(r"\.write_bytes\(").unwrap(),
    ];

    // Bash/进程执行能力模式
    static ref RE_BASH: Vec<Regex> = vec![
        Regex::new(r"subprocess\.(run|call|Popen)").unwrap(),
        Regex::new(r"os\.system").unwrap(),
        Regex::new(r"shell\s*=\s*True").unwrap(),
        Regex::new(r"os\.popen").unwrap(),
    ];

    // Grep/正则能力模式
    static ref RE_GREP: Vec<Regex> = vec![
        Regex::new(r"re\.(search|findall|match|finditer|sub)\(").unwrap(),
        Regex::new(r"\bgrep\b").unwrap(),
    ];

    // Glob 能力模式
    static ref RE_GLOB: Vec<Regex> = vec![
        Regex::new(r"glob\.glob").unwrap(),
        Regex::new(r"\.rglob\(").unwrap(),
        Regex::new(r"\.glob\(").unwrap(),
        Regex::new(r"fnmatch\.").unwrap(),
    ];

    // Network 能力模式（始终检查，不区分 allowed_tools）
    static ref RE_NETWORK: Vec<Regex> = vec![
        Regex::new(r"requests\.(get|post|put|delete)").unwrap(),
        Regex::new(r"urllib").unwrap(),
        Regex::new(r"httpx").unwrap(),
        Regex::new(r"aiohttp").unwrap(),
        Regex::new(r"socket\.connect").unwrap(),
    ];

    static ref RE_HTTP_URL: Regex = Regex::new(r#"https?://[^\s"'`]+"#).unwrap();

    // 描述泛化模式
    static ref RE_GENERIC_DESC: Vec<Regex> = vec![
        Regex::new(r"(?i)^help\s").unwrap(),
        Regex::new(r"(?i)^assistant$").unwrap(),
        Regex::new(r"(?i)^helper$").unwrap(),
        Regex::new(r"(?i)do\s+(anything|everything)").unwrap(),
        Regex::new(r"(?i)general\s+purpose").unwrap(),
        Regex::new(r"(?i)universal").unwrap(),
    ];

    // 简单功能词（用于 SOCIAL_ENG_MISLEADING_DESC）
    static ref RE_SIMPLE_FEATURE: Vec<Regex> = vec![
        Regex::new(r"(?i)calculator").unwrap(),
        Regex::new(r"(?i)format").unwrap(),
        Regex::new(r"(?i)template").unwrap(),
        Regex::new(r"(?i)style").unwrap(),
        Regex::new(r"(?i)lint").unwrap(),
        Regex::new(r"(?i)converter").unwrap(),
        Regex::new(r"(?i)parser").unwrap(),
    ];
}

// ── 能力名称映射 ──

/// 将 allowed_tools 中的能力名称规范化为统一标识
fn normalize_tool_name(tool: &str) -> String {
    tool.to_lowercase().replace(['-', '_', ' '], "")
}

/// 检查 allowed_tools 中是否声明了某种能力
fn has_tool(tools: &[String], capability: &str) -> bool {
    let cap_norm = normalize_tool_name(capability);
    tools.iter().any(|t| {
        let t_norm = normalize_tool_name(t);
        t_norm == cap_norm
            || t_norm.contains(&cap_norm)
            || cap_norm.contains(&t_norm)
    })
}

// ── 代码内容匹配 ──

/// 检查代码内容是否匹配指定的正则模式列表
fn matches_any(content: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|re| re.is_match(content))
}

/// 代码是否使用网络（排除仅 localhost 的开发/健康检查流量）
fn uses_network_excluding_localhost(content: &str) -> bool {
    if !matches_any(content, &RE_NETWORK) {
        return false;
    }
    for m in RE_HTTP_URL.find_iter(content) {
        let url = m.as_str();
        if !url.contains("127.0.0.1") && !url.contains("localhost") {
            return true;
        }
    }
    content.contains("socket.")
        && !content.contains("127.0.0.1")
        && !content.contains("localhost")
}

/// 创建一个 Finding 实例（consistency_checker 专用）
///
/// 使用 sha2 生成稳定的 finding ID：SHA256(rule_id + file + line)[:16]
fn make_finding(
    rule_id: &str,
    severity: IssueSeverity,
    description: String,
    file_path: Option<String>,
    line_number: Option<usize>,
) -> Finding {
    let id_input = format!(
        "{}|{}|{}",
        rule_id,
        file_path.as_deref().unwrap_or(""),
        line_number
            .map(|l| l.to_string())
            .unwrap_or_default()
    );
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let id = hash[..16].to_string();

    let (title, category) = match rule_id {
        "ALLOWED_TOOLS_READ_VIOLATION" => ("Undeclared Read capability", ThreatCategory::PolicyViolation),
        "ALLOWED_TOOLS_WRITE_VIOLATION" => ("Undeclared Write capability", ThreatCategory::PolicyViolation),
        "ALLOWED_TOOLS_BASH_VIOLATION" => ("Undeclared Bash capability", ThreatCategory::PolicyViolation),
        "ALLOWED_TOOLS_GREP_VIOLATION" => ("Undeclared Grep capability", ThreatCategory::PolicyViolation),
        "ALLOWED_TOOLS_GLOB_VIOLATION" => ("Undeclared Glob capability", ThreatCategory::PolicyViolation),
        "ALLOWED_TOOLS_NETWORK_USAGE" => ("Network usage detected", ThreatCategory::Network),
        "TOOL_ABUSE_UNDECLARED_NETWORK" => ("Undeclared network usage", ThreatCategory::PolicyViolation),
        "SOCIAL_ENG_MISLEADING_DESC" => ("Misleading description", ThreatCategory::SocialEngineering),
        "TRIGGER_OVERLY_GENERIC" => ("Overly generic description", ThreatCategory::SocialEngineering),
        "TRIGGER_DESCRIPTION_TOO_SHORT" => ("Description too short", ThreatCategory::SocialEngineering),
        "TRIGGER_VAGUE_DESCRIPTION" => ("Vague description", ThreatCategory::SocialEngineering),
        "TRIGGER_KEYWORD_BAITING" => ("Keyword baiting detected", ThreatCategory::SocialEngineering),
        _ => ("Consistency violation", ThreatCategory::PolicyViolation),
    };

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category,
        severity,
        title: title.to_string(),
        description,
        file_path,
        line_number,
        snippet: None,
        remediation: Some("Review and correct the skill consistency per policy".to_string()),
        analyzer: ANALYZER_NAME.to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some("consistency_checker".to_string()),
            ..Default::default()
        }),
    }
}

/// 读取脚本文件内容，返回 (文件路径, 内容) 列表
fn read_script_contents(ctx: &SkillContext) -> Vec<(String, String)> {
    let mut contents = Vec::new();
    for file in &ctx.files {
        if file.file_type == SkillFileType::Script && !file.is_binary {
            if let Ok(content) = std::fs::read_to_string(&file.absolute_path) {
                let rel = file.relative_path.to_string_lossy().to_string();
                contents.push((rel, content));
            }
        }
    }
    contents
}

// ── 公共接口 ──

/// 对 SkillContext 执行一致性检查，返回所有 Finding
pub fn check(ctx: &SkillContext) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(check_allowed_tools(ctx));
    findings.extend(check_manifest_consistency(ctx));
    findings.extend(check_description_quality(ctx));
    findings
}

/// 检查 allowed-tools 声明与代码实际行为的一致性
///
/// 只扫描脚本文件（SkillFileType::Script），不扫描文档。
/// 对每种能力：若该能力在 allowed_tools 中声明则跳过，
/// 若不在声明中但代码匹配了对应模式则产生 finding。
pub fn check_allowed_tools(ctx: &SkillContext) -> Vec<Finding> {
    let mut findings = Vec::new();

    let allowed_tools = match ctx.manifest.as_ref() {
        Some(m) if !m.allowed_tools.is_empty() => &m.allowed_tools,
        _ => return findings, // 无声明则跳过
    };

    let script_contents = read_script_contents(ctx);

    // 定义所有能力检查项：(rule_id, severity, capability_name, patterns, threat_category)
    struct CapabilityCheck<'a> {
        rule_id: &'a str,
        severity: IssueSeverity,
        capability: &'a str,
        patterns: &'a [Regex],
        category: ThreatCategory,
    }

    let checks = [
        CapabilityCheck {
            rule_id: "ALLOWED_TOOLS_READ_VIOLATION",
            severity: IssueSeverity::Medium,
            capability: "Read",
            patterns: &RE_READ,
            category: ThreatCategory::PolicyViolation,
        },
        CapabilityCheck {
            rule_id: "ALLOWED_TOOLS_WRITE_VIOLATION",
            severity: IssueSeverity::Medium,
            capability: "Write",
            patterns: &RE_WRITE,
            category: ThreatCategory::PolicyViolation,
        },
        CapabilityCheck {
            rule_id: "ALLOWED_TOOLS_BASH_VIOLATION",
            severity: IssueSeverity::High,
            capability: "Bash",
            patterns: &RE_BASH,
            category: ThreatCategory::PolicyViolation,
        },
        CapabilityCheck {
            rule_id: "ALLOWED_TOOLS_GREP_VIOLATION",
            severity: IssueSeverity::Low,
            capability: "Grep",
            patterns: &RE_GREP,
            category: ThreatCategory::PolicyViolation,
        },
        CapabilityCheck {
            rule_id: "ALLOWED_TOOLS_GLOB_VIOLATION",
            severity: IssueSeverity::Low,
            capability: "Glob",
            patterns: &RE_GLOB,
            category: ThreatCategory::PolicyViolation,
        },
        // Network 始终检查（不查看 allowed_tools）
        CapabilityCheck {
            rule_id: "ALLOWED_TOOLS_NETWORK_USAGE",
            severity: IssueSeverity::Medium,
            capability: "Network",
            patterns: &RE_NETWORK,
            category: ThreatCategory::Network,
        },
    ];

    for check in &checks {
        // Network 始终检查，其他能力跳过已声明的
        if check.capability != "Network" && has_tool(allowed_tools, check.capability) {
            continue;
        }

        for (file_path, content) in &script_contents {
            let matched = if check.capability == "Network" {
                uses_network_excluding_localhost(content)
            } else {
                matches_any(content, check.patterns)
            };
            if matched {
                findings.push(make_finding(
                    check.rule_id,
                    check.severity,
                    format!(
                        "Code uses {} capability in '{}' but it is not declared in allowed-tools",
                        check.capability, file_path
                    ),
                    Some(file_path.clone()),
                    None,
                ));
            }
        }
    }

    findings
}

/// 检查 manifest 元数据与代码行为的一致性
pub fn check_manifest_consistency(ctx: &SkillContext) -> Vec<Finding> {
    let mut findings = Vec::new();

    let manifest = match ctx.manifest.as_ref() {
        Some(m) => m,
        None => return findings,
    };

    let script_contents = read_script_contents(ctx);

    // 检查代码是否使用网络
    let code_uses_network = script_contents
        .iter()
        .any(|(_, content)| uses_network_excluding_localhost(content));

    if code_uses_network {
        // TOOL_ABUSE_UNDECLARED_NETWORK: 代码使用网络但 compatibility 不含 network/internet
        let compat_has_network = manifest.compatibility.values().any(|v| {
            let v_lower = v.to_lowercase();
            v_lower.contains("network") || v_lower.contains("internet")
        });
        // 也检查 compatibility 的 keys
        let compat_key_has_network = manifest.compatibility.keys().any(|k| {
            let k_lower = k.to_lowercase();
            k_lower.contains("network") || k_lower.contains("internet")
        });

        if !compat_has_network && !compat_key_has_network {
            findings.push(make_finding(
                "TOOL_ABUSE_UNDECLARED_NETWORK",
                IssueSeverity::Medium,
                "Code uses network but manifest compatibility does not declare network/internet support"
                    .to_string(),
                None,
                None,
            ));
        }

        // SOCIAL_ENG_MISLEADING_DESC: 描述含简单功能词但代码使用网络
        let desc_has_simple_feature = RE_SIMPLE_FEATURE.iter().any(|re| {
            re.is_match(&manifest.description)
        });

        if desc_has_simple_feature {
            findings.push(make_finding(
                "SOCIAL_ENG_MISLEADING_DESC",
                IssueSeverity::Medium,
                "Description suggests simple offline functionality (calculator/format/template/etc.) but code uses network"
                    .to_string(),
                None,
                None,
            ));
        }
    }

    findings
}

/// 检查 description 质量
pub fn check_description_quality(ctx: &SkillContext) -> Vec<Finding> {
    let mut findings = Vec::new();

    let description = match ctx.manifest.as_ref() {
        Some(m) if !m.description.is_empty() => &m.description,
        _ => return findings,
    };

    // TRIGGER_OVERLY_GENERIC: 描述匹配泛化模式
    if RE_GENERIC_DESC.iter().any(|re| re.is_match(description)) {
        findings.push(make_finding(
            "TRIGGER_OVERLY_GENERIC",
            IssueSeverity::Medium,
            format!(
                "Description is overly generic: '{}'",
                truncate_str(description, 80)
            ),
            None,
            None,
        ));
    }

    // TRIGGER_DESCRIPTION_TOO_SHORT: 描述单词数 < 5
    let word_count = description.split_whitespace().count();
    if word_count < 5 {
        findings.push(make_finding(
            "TRIGGER_DESCRIPTION_TOO_SHORT",
            IssueSeverity::Low,
            format!(
                "Description is too short ({} words, minimum 5): '{}'",
                word_count,
                truncate_str(description, 80)
            ),
            None,
            None,
        ));
    }

    // TRIGGER_VAGUE_DESCRIPTION: 泛化词占比 > 40% 且具体技术词 < 2
    let vague_words = [
        "help", "helper", "assistant", "tool", "utility", "useful",
        "general", "simple", "basic", "easy", "quick", "fast",
        "good", "nice", "best", "great", "smart", "powerful",
        "automate", "manage", "handle", "process", "support",
    ];
    let tech_words = [
        "api", "http", "json", "yaml", "docker", "kubernetes", "aws",
        "azure", "gcp", "sql", "database", "regex", "parser", "compiler",
        "terraform", "ansible", "ci/cd", "git", "ssh", "ssl", "tls",
        "rest", "grpc", "websocket", "oauth", "jwt", "encryption",
        "hash", "token", "webhook", "microservice", "container",
        "linux", "python", "javascript", "typescript", "rust", "go",
        "react", "vue", "angular", "node", "express", "flask", "django",
    ];

    let desc_lower = description.to_lowercase();
    let words: Vec<&str> = desc_lower.split_whitespace().collect();
    let total_words = words.len();

    if total_words > 0 {
        let vague_count = words
            .iter()
            .filter(|w| vague_words.contains(w))
            .count();
        let tech_count = words
            .iter()
            .filter(|w| tech_words.contains(w))
            .count();

        let vague_ratio = vague_count as f64 / total_words as f64;
        if vague_ratio > 0.4 && tech_count < 2 {
            findings.push(make_finding(
                "TRIGGER_VAGUE_DESCRIPTION",
                IssueSeverity::Low,
                format!(
                    "Description is vague: {:.0}% generic words, {} technical words",
                    vague_ratio * 100.0,
                    tech_count
                ),
                None,
                None,
            ));
        }
    }

    // TRIGGER_KEYWORD_BAITING: 描述含 8+ 个逗号分隔关键词（非 "example/such as" 引导）
    let comma_count = description.matches(',').count();
    if comma_count >= 7 {
        // 8+ 个逗号分隔项（7 个逗号 = 8 个项）
        // 检查是否为 "example" 或 "such as" 引导的合法列举
        let desc_lower_check = description.to_lowercase();
        let is_example_list = desc_lower_check.contains("example")
            || desc_lower_check.contains("such as")
            || desc_lower_check.contains("including")
            || desc_lower_check.contains("e.g.");

        if !is_example_list {
            findings.push(make_finding(
                "TRIGGER_KEYWORD_BAITING",
                IssueSeverity::Medium,
                format!(
                    "Description contains {} comma-separated keywords, possibly keyword baiting",
                    comma_count + 1
                ),
                None,
                None,
            ));
        }
    }

    findings
}

/// 截断字符串到指定长度
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

// ── 单元测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::policy::ScanPolicy;
    use crate::security::skill_context::{ScanMode, SkillContext, SkillManifest};
    use std::path::PathBuf;

    fn make_test_ctx(manifest: Option<SkillManifest>) -> SkillContext {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(
            ScanMode::Directory,
            Some(PathBuf::from("/tmp/test-skill")),
            policy,
        );
        ctx.manifest = manifest;
        ctx
    }

    #[test]
    fn test_allowed_tools_empty_no_findings() {
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "A test skill for testing".to_string(),
            allowed_tools: vec![],
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check_allowed_tools(&ctx);
        assert!(findings.is_empty(), "Empty allowed_tools should produce no findings");
    }

    #[test]
    fn test_allowed_tools_no_manifest_no_findings() {
        let ctx = make_test_ctx(None);
        let findings = check_allowed_tools(&ctx);
        assert!(findings.is_empty(), "No manifest should produce no findings");
    }

    #[test]
    fn test_allowed_tools_read_declared_no_violation() {
        // allowed_tools 声明了 Read，代码有 open('r') → 不触发
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: A valid description for testing\nallowed-tools:\n  - Read\n---\n\nBody.";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();
        std::fs::write(dir_path.join("helper.py"), "with open('data.txt', 'r') as f:\n    content = f.read()").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = check_allowed_tools(&ctx);

        let read_violations: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "ALLOWED_TOOLS_READ_VIOLATION")
            .collect();
        assert!(
            read_violations.is_empty(),
            "Read declared → should not trigger READ_VIOLATION, got: {:?}",
            read_violations
        );
    }

    #[test]
    fn test_allowed_tools_bash_not_declared_triggers() {
        // allowed_tools 只声明 Read，代码有 subprocess.run → 触发 BASH_VIOLATION
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: A valid description for testing\nallowed-tools:\n  - Read\n---\n\nBody.";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();
        std::fs::write(dir_path.join("run.py"), "import subprocess\nsubprocess.run(['ls', '-la'])").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = check_allowed_tools(&ctx);

        let bash_violations: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "ALLOWED_TOOLS_BASH_VIOLATION")
            .collect();
        assert!(
            !bash_violations.is_empty(),
            "Bash not declared but code uses subprocess.run → should trigger BASH_VIOLATION"
        );
        assert_eq!(bash_violations[0].severity, IssueSeverity::High);
    }

    #[test]
    fn test_network_usage_always_checked() {
        // 即使 allowed_tools 声明了 Network，仍然产生 ALLOWED_TOOLS_NETWORK_USAGE
        // （按需求表，Network 始终检查）
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: A valid description for testing\nallowed-tools:\n  - Read\n  - Network\n---\n\nBody.";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();
        std::fs::write(dir_path.join("fetch.py"), "import requests\nrequests.get('https://example.com')").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = check_allowed_tools(&ctx);

        // Network 始终检查，即使声明了也触发
        let network_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "ALLOWED_TOOLS_NETWORK_USAGE")
            .collect();
        assert!(
            !network_findings.is_empty(),
            "Network usage should always be reported"
        );
    }

    #[test]
    fn test_manifest_network_undeclared() {
        // 代码使用网络但 compatibility 未声明 → 触发 TOOL_ABUSE_UNDECLARED_NETWORK
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: A valid description for testing\ncompatibility:\n  platform: linux\n---\n\nBody.";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();
        std::fs::write(dir_path.join("fetch.py"), "import requests\nrequests.get('https://example.com')").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = check_manifest_consistency(&ctx);

        let network_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TOOL_ABUSE_UNDECLARED_NETWORK")
            .collect();
        assert!(
            !network_findings.is_empty(),
            "Code uses network but compatibility doesn't declare it → should trigger TOOL_ABUSE_UNDECLARED_NETWORK"
        );
    }

    #[test]
    fn test_manifest_network_declared_no_finding() {
        // 代码使用网络且 compatibility 声明了 network → 不触发
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: A valid description for testing\ncompatibility:\n  platform: linux\n  network: required\n---\n\nBody.";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();
        std::fs::write(dir_path.join("fetch.py"), "import requests\nrequests.get('https://example.com')").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = check_manifest_consistency(&ctx);

        let network_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TOOL_ABUSE_UNDECLARED_NETWORK")
            .collect();
        assert!(
            network_findings.is_empty(),
            "Network declared in compatibility → should not trigger TOOL_ABUSE_UNDECLARED_NETWORK"
        );
    }

    #[test]
    fn test_description_too_short() {
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "short".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check_description_quality(&ctx);

        let short_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TRIGGER_DESCRIPTION_TOO_SHORT")
            .collect();
        assert!(
            !short_findings.is_empty(),
            "Description with < 5 words should trigger TRIGGER_DESCRIPTION_TOO_SHORT"
        );
    }

    #[test]
    fn test_description_overly_generic() {
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "helper".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check_description_quality(&ctx);

        let generic_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TRIGGER_OVERLY_GENERIC")
            .collect();
        assert!(
            !generic_findings.is_empty(),
            "Description 'helper' should trigger TRIGGER_OVERLY_GENERIC"
        );
    }

    #[test]
    fn test_description_keyword_baiting() {
        // 8+ 个逗号分隔关键词（7 个逗号 = 8 个项），非 example/such as 引导
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "tool for api, http, json, yaml, docker, kubernetes, aws, azure, gcp".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check_description_quality(&ctx);

        let baiting_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TRIGGER_KEYWORD_BAITING")
            .collect();
        assert!(
            !baiting_findings.is_empty(),
            "Description with 9 comma-separated keywords should trigger TRIGGER_KEYWORD_BAITING"
        );
    }

    #[test]
    fn test_description_keyword_baiting_with_example_no_finding() {
        // 含 "example" 引导的列举不应触发
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "Supports many formats, for example: api, http, json, yaml, docker, kubernetes, aws, azure".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check_description_quality(&ctx);

        let baiting_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TRIGGER_KEYWORD_BAITING")
            .collect();
        assert!(
            baiting_findings.is_empty(),
            "Description with 'example' keyword list should not trigger TRIGGER_KEYWORD_BAITING"
        );
    }

    #[test]
    fn test_description_vague_high_ratio() {
        // 泛化词占比 > 40% 且具体技术词 < 2
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "A good helper tool for general simple basic easy quick useful".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check_description_quality(&ctx);

        let vague_findings: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "TRIGGER_VAGUE_DESCRIPTION")
            .collect();
        assert!(
            !vague_findings.is_empty(),
            "Description with high vague word ratio should trigger TRIGGER_VAGUE_DESCRIPTION"
        );
    }

    #[test]
    fn test_check_main_function() {
        // 测试 check 主函数是否调用了所有子检查
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "helper".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check(&ctx);

        // 应该有 TRIGGER_OVERLY_GENERIC 和 TRIGGER_DESCRIPTION_TOO_SHORT
        assert!(
            findings.iter().any(|f| f.rule_id == "TRIGGER_OVERLY_GENERIC"),
            "check() should include TRIGGER_OVERLY_GENERIC"
        );
    }

    #[test]
    fn test_analyzer_name() {
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "helper".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check(&ctx);

        for finding in &findings {
            assert_eq!(
                finding.analyzer, ANALYZER_NAME,
                "All findings should have analyzer = 'consistency_checker'"
            );
        }
    }

    #[test]
    fn test_finding_category() {
        let manifest = Some(SkillManifest {
            name: "test".to_string(),
            description: "helper".to_string(),
            ..Default::default()
        });
        let ctx = make_test_ctx(manifest);
        let findings = check(&ctx);

        for finding in &findings {
            // consistency_checker 使用 PolicyViolation 或 SocialEngineering 或 Network
            assert!(
                matches!(
                    finding.category,
                    ThreatCategory::PolicyViolation
                        | ThreatCategory::SocialEngineering
                        | ThreatCategory::Network
                ),
                "Finding category should be PolicyViolation, SocialEngineering, or Network, got: {:?}",
                finding.category
            );
        }
    }
}
