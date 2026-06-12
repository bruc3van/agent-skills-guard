//! YAML 规则包加载器
//!
//! 负责从 YAML 字符串加载规则包，编译正则模式，
//! 并与内置规则合并为统一规则列表。

use super::{RulePack, YamlRule};
use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;

/// 编译后的 YAML 规则（正则已编译）
#[derive(Debug, Clone)]
pub struct CompiledYamlRule {
    pub id: String,
    pub compiled_patterns: Vec<Regex>,
    pub compiled_exclude_patterns: Vec<Regex>,
    pub rule: YamlRule,
}

fn compile_rule_regex(pattern: &str) -> Result<Regex> {
    let needs_multiline = pattern.contains('^') || pattern.contains('$');
    let compiled = if needs_multiline && !pattern.contains("(?m)") && !pattern.contains("(?s)") {
        format!("(?m){pattern}")
    } else {
        pattern.to_string()
    };
    let mut builder = RegexBuilder::new(&compiled);
    builder.size_limit(10_000_000); // 10MB limit to prevent ReDoS
    if needs_multiline {
        builder.multi_line(true);
    }
    builder.build().with_context(|| format!("Invalid regex pattern: {pattern}"))
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
            let re = compile_rule_regex(pattern).with_context(|| {
                format!("Invalid regex pattern in rule {}: {}", rule.id, pattern)
            })?;
            compiled_patterns.push(re);
        }

        // 编译排除模式
        let mut compiled_exclude_patterns = Vec::with_capacity(rule.exclude_patterns.len());
        for pattern in &rule.exclude_patterns {
            let re = compile_rule_regex(pattern).with_context(|| {
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

/// Cisco parity 补充规则（与 reference/skill_scanner signatures 对齐）
const CISCO_PARITY_RULES_YAML: &str =
    include_str!("../../../resources/security/packs/core/signatures/cisco_parity_signatures.yaml");

fn load_merged_builtin_rules() -> Vec<CompiledYamlRule> {
    let mut rules =
        load_rule_pack(CORE_RULES_YAML).expect("Failed to compile built-in YAML rule pack");
    let cisco = load_rule_pack(CISCO_PARITY_RULES_YAML)
        .expect("Failed to compile Cisco parity YAML rule pack");
    let mut seen: HashSet<String> = rules.iter().map(|r| r.id.clone()).collect();
    for rule in cisco {
        assert!(
            seen.insert(rule.id.clone()),
            "Duplicate rule ID across rule packs: {}",
            rule.id
        );
        rules.push(rule);
    }
    rules
}

/// 内置编译后的规则包（core + Cisco parity）
static BUILTIN_COMPILED_RULES: std::sync::LazyLock<Vec<CompiledYamlRule>> =
    std::sync::LazyLock::new(load_merged_builtin_rules);

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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate rule ID"));
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
        assert!(!rules.is_empty(), "Builtin YAML rules should not be empty");
        // 验证规则数量与 Rust 硬编码规则一致（约 72 条）
        assert!(
            rules.len() >= 65,
            "Expected at least 65 builtin rules, got {}",
            rules.len()
        );
        // 验证关键规则存在
        let rule_ids: Vec<&str> = rules.iter().map(|r| r.id.as_str()).collect();
        assert!(
            rule_ids.contains(&"RM_RF_ROOT"),
            "Should contain RM_RF_ROOT"
        );
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

    #[test]
    fn test_all_rules_have_metadata_origin() {
        let rules = get_builtin_compiled_rules();
        for rule in rules {
            let meta = rule.rule.metadata.as_ref().unwrap_or_else(|| {
                panic!("Rule {} is missing metadata field", rule.id)
            });
            let meta_map = meta.as_mapping().unwrap_or_else(|| {
                panic!("Rule {} metadata should be a mapping", rule.id)
            });
            let origin = meta_map
                .get(&serde_yaml::Value::String("origin".into()))
                .unwrap_or_else(|| panic!("Rule {} missing metadata.origin", rule.id));
            let origin_str = origin.as_str().unwrap();
            assert!(
                origin_str == "core" || origin_str == "cisco_parity",
                "Rule {} has invalid metadata.origin: {}",
                rule.id,
                origin_str
            );
        }
    }

    #[test]
    fn test_cisco_parity_rules_have_source_ref() {
        let rules = get_builtin_compiled_rules();
        for rule in rules {
            let meta = rule.rule.metadata.as_ref().unwrap_or_else(|| {
                panic!("Rule {} missing metadata", rule.id)
            });
            let meta_map = meta.as_mapping().unwrap();
            let origin = meta_map
                .get(&serde_yaml::Value::String("origin".into()))
                .unwrap_or_else(|| panic!("Rule {} missing metadata.origin", rule.id));
            if origin.as_str() != Some("cisco_parity") {
                continue; // core 规则不需要 source_ref
            }
            // Cisco parity 规则必须有 source_ref
            assert!(
                meta_map.contains_key(&serde_yaml::Value::String("source_ref".into())),
                "Cisco parity rule {} missing metadata.source_ref",
                rule.id
            );
        }
    }

    #[test]
    fn test_metadata_fp_risk_valid() {
        let rules = get_builtin_compiled_rules();
        let valid_fp_risks = ["low", "medium", "high"];
        for rule in rules {
            let meta = rule.rule.metadata.as_ref().unwrap_or_else(|| {
                panic!("Rule {} missing metadata", rule.id)
            });
            let meta_map = meta.as_mapping().unwrap();
            let fp_risk = meta_map
                .get(&serde_yaml::Value::String("fp_risk".into()))
                .unwrap_or_else(|| panic!("Rule {} missing metadata.fp_risk", rule.id));
            let fp_str = fp_risk.as_str().unwrap();
            assert!(
                valid_fp_risks.contains(&fp_str),
                "Rule {} has invalid metadata.fp_risk: {}",
                rule.id,
                fp_str
            );
        }
    }

    #[test]
    fn test_hard_trigger_rules_have_high_severity_and_confidence() {
        let rules = get_builtin_compiled_rules();
        for rule in rules {
            if !rule.rule.hard_trigger {
                continue;
            }
            // hard_trigger 规则 severity 必须是 Critical 或 High
            assert!(
                matches!(
                    rule.rule.severity,
                    crate::models::security::IssueSeverity::Critical
                        | crate::models::security::IssueSeverity::High
                ),
                "Hard trigger rule {} should have Critical or High severity, got {:?}",
                rule.id,
                rule.rule.severity
            );
            // hard_trigger 规则 confidence 不能是 Low
            assert!(
                !rule.rule.confidence.eq_ignore_ascii_case("low"),
                "Hard trigger rule {} should not have Low confidence",
                rule.id
            );
        }
    }

    #[test]
    fn test_file_types_extensions_start_with_dot() {
        let rules = get_builtin_compiled_rules();
        for rule in rules {
            for ext in &rule.rule.file_types {
                assert!(
                    ext.starts_with('.'),
                    "Rule {} file_types entry '{}' should start with '.'",
                    rule.id,
                    ext
                );
            }
        }
    }

    #[test]
    fn test_suppress_if_matched_no_self_reference() {
        let rules = get_builtin_compiled_rules();
        for rule in rules {
            for suppressed_id in &rule.rule.suppress_if_matched {
                assert_ne!(
                    suppressed_id, &rule.id,
                    "Rule {} suppress_if_matched references itself",
                    rule.id
                );
            }
        }
    }

}
