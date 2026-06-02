//! YAML 规则包加载器
//!
//! 负责从 YAML 字符串加载规则包，编译正则模式，
//! 并与内置规则合并为统一规则列表。

use super::{RulePack, YamlRule};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashSet;

/// 编译后的 YAML 规则（正则已编译）
#[derive(Debug, Clone)]
pub struct CompiledYamlRule {
    pub id: String,
    pub compiled_patterns: Vec<Regex>,
    pub compiled_exclude_patterns: Vec<Regex>,
    pub rule: YamlRule,
}

/// 加载并编译规则包
pub fn load_rule_pack(yaml: &str) -> Result<Vec<CompiledYamlRule>> {
    let pack: RulePack =
        serde_yaml::from_str(yaml).with_context(|| "Failed to parse rule pack YAML")?;

    compile_rule_pack(pack)
}

/// 编译规则包中的所有规则
pub fn compile_rule_pack(pack: RulePack) -> Result<Vec<CompiledYamlRule>> {
    let mut compiled = Vec::with_capacity(pack.rules.len());
    let mut seen_ids = HashSet::new();

    for rule in pack.rules {
        // 检查重复 ID
        if !seen_ids.insert(rule.id.clone()) {
            anyhow::bail!("Duplicate rule ID in pack: {}", rule.id);
        }

        // 编译正则模式
        let mut compiled_patterns = Vec::with_capacity(rule.patterns.len());
        for pattern in &rule.patterns {
            let re = Regex::new(pattern)
                .with_context(|| format!("Invalid regex pattern in rule {}: {}", rule.id, pattern))?;
            compiled_patterns.push(re);
        }

        // 编译排除模式
        let mut compiled_exclude_patterns = Vec::with_capacity(rule.exclude_patterns.len());
        for pattern in &rule.exclude_patterns {
            let re = Regex::new(pattern).with_context(|| {
                format!(
                    "Invalid exclude regex pattern in rule {}: {}",
                    rule.id, pattern
                )
            })?;
            compiled_exclude_patterns.push(re);
        }

        compiled.push(CompiledYamlRule {
            id: rule.id.clone(),
            compiled_patterns,
            compiled_exclude_patterns,
            rule,
        });
    }

    Ok(compiled)
}

/// 加载内置规则包（编译时嵌入的 YAML）
const CORE_RULES_YAML: &str =
    include_str!("../../../resources/security/packs/core/signatures/core_rules.yaml");

lazy_static::lazy_static! {
    /// 内置编译后的规则包
    static ref BUILTIN_COMPILED_RULES: Vec<CompiledYamlRule> = {
        load_rule_pack(CORE_RULES_YAML)
            .expect("Failed to compile built-in YAML rule pack")
    };
}

/// 获取内置编译后的规则包
pub fn get_builtin_compiled_rules() -> &'static Vec<CompiledYamlRule> {
    &BUILTIN_COMPILED_RULES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_simple_rule_pack() {
        let yaml = r#"
name: test
version: "1.0"
rules:
  - id: TEST_RULE
    category: Destructive
    severity: Critical
    weight: 100
    hard_trigger: true
    patterns:
      - "rm -rf /"
    description: "Test rule"
    remediation: "Don't do this"
"#;
        let rules = load_rule_pack(yaml).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "TEST_RULE");
        assert!(rules[0].rule.hard_trigger);
    }

    #[test]
    fn test_duplicate_rule_id_fails() {
        let yaml = r#"
name: test
rules:
  - id: DUP
    category: Destructive
    severity: Critical
    patterns: ["test"]
    description: "First"
  - id: DUP
    category: Network
    severity: Low
    patterns: ["test2"]
    description: "Second"
"#;
        let result = load_rule_pack(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate rule ID"));
    }

    #[test]
    fn test_invalid_regex_fails() {
        let yaml = r#"
name: test
rules:
  - id: BAD_REGEX
    category: Destructive
    severity: Critical
    patterns: ["[invalid"]
    description: "Bad regex"
"#;
        let result = load_rule_pack(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_builtin_yaml_rules() {
        let rules = get_builtin_compiled_rules();
        // 验证内置 YAML 规则已加载
        assert!(
            !rules.is_empty(),
            "Builtin YAML rules should not be empty"
        );
        // 验证规则数量与 Rust 硬编码规则一致（约 72 条）
        assert!(
            rules.len() >= 65,
            "Expected at least 65 builtin rules, got {}",
            rules.len()
        );
        // 验证关键规则存在
        let rule_ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        assert!(rule_ids.contains(&"RM_RF_ROOT"), "Should contain RM_RF_ROOT");
        assert!(
            rule_ids.contains(&"CURL_PIPE_SH"),
            "Should contain CURL_PIPE_SH"
        );
        assert!(
            rule_ids.contains(&"REVERSE_SHELL"),
            "Should contain REVERSE_SHELL"
        );
        assert!(
            rule_ids.contains(&"PRIVATE_KEY"),
            "Should contain PRIVATE_KEY"
        );
    }

    #[test]
    fn test_builtin_yaml_rules_have_correct_fields() {
        let rules = get_builtin_compiled_rules();
        for rule in rules {
            // 每条规则至少有一个 pattern
            assert!(
                !rule.compiled_patterns.is_empty(),
                "Rule {} should have at least one pattern",
                rule.id
            );
            // 严重程度有效
            assert!(
                matches!(
                    rule.rule.severity,
                    crate::models::security::IssueSeverity::Critical
                        | crate::models::security::IssueSeverity::High
                        | crate::models::security::IssueSeverity::Medium
                        | crate::models::security::IssueSeverity::Low
                        | crate::models::security::IssueSeverity::Info
                ),
                "Rule {} has invalid severity",
                rule.id
            );
            // 硬触发规则应有高权重
            if rule.rule.hard_trigger {
                assert!(
                    rule.rule.weight >= 50,
                    "Hard trigger rule {} should have weight >= 50, got {}",
                    rule.id,
                    rule.rule.weight
                );
            }
        }
    }
}
