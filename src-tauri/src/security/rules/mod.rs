//! 安全规则体系
//!
//! - `builtin_compat`: 现有硬编码规则（迁移期兼容）
//! - `loader`: YAML 规则包加载
//! - `pattern_engine`: 增强版规则匹配引擎

pub mod builtin_compat;
pub mod loader;
pub mod pattern_engine;

// 从 builtin_compat 重新导出（保持向后兼容）
pub use builtin_compat::{Category, Confidence, PatternRule, SecurityRules, Severity};

use crate::models::security::{IssueSeverity, ThreatCategory};
use serde::{Deserialize, Serialize};

/// YAML 规则定义（从 YAML 文件加载）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlRule {
    /// 规则 ID（唯一标识）
    pub id: String,
    /// 威胁分类
    pub category: ThreatCategory,
    /// 严重程度
    pub severity: IssueSeverity,
    /// 权重（0-100，参与评分扣分）
    #[serde(default)]
    pub weight: i32,
    /// 置信度（影响非硬触发规则的评分系数）
    #[serde(default = "default_confidence")]
    pub confidence: String,
    /// 是否为硬触发规则（匹配即阻断）
    #[serde(default)]
    pub hard_trigger: bool,
    /// 正则模式列表（一条规则可有多个 pattern）
    pub patterns: Vec<String>,
    /// 排除模式（先排除再命中）
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// 适用的文件扩展名（空表示匹配全部）
    #[serde(default)]
    pub file_types: Vec<String>,
    /// 同行命中时抑制当前规则的规则 ID 列表
    #[serde(default)]
    pub suppress_if_matched: Vec<String>,
    /// 规则描述
    pub description: String,
    /// 修复建议
    #[serde(default)]
    pub remediation: String,
    /// CWE 编号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwe_id: Option<String>,
    /// 扩展元数据
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_yaml::Value>,
}

fn default_confidence() -> String {
    "Medium".to_string()
}

impl YamlRule {
    /// 获取 Confidence 枚举值
    pub fn confidence_enum(&self) -> Confidence {
        match self.confidence.as_str() {
            "High" => Confidence::High,
            "Medium" => Confidence::Medium,
            "Low" => Confidence::Low,
            _ => Confidence::Medium,
        }
    }
}

/// 规则包（YAML 文件的集合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePack {
    /// 规则包名称
    pub name: String,
    /// 规则包版本
    #[serde(default)]
    pub version: String,
    /// 规则包描述
    #[serde(default)]
    pub description: String,
    /// 规则列表
    pub rules: Vec<YamlRule>,
}

/// 统一的规则表示（合并 builtin 和 YAML 规则）
#[derive(Debug, Clone)]
pub enum UnifiedRule {
    /// 内置硬编码规则
    Builtin(&'static PatternRule),
    /// YAML 加载的规则
    Yaml(YamlRule),
}

impl UnifiedRule {
    pub fn id(&self) -> &str {
        match self {
            UnifiedRule::Builtin(r) => r.id,
            UnifiedRule::Yaml(r) => &r.id,
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            UnifiedRule::Builtin(r) => r.severity,
            UnifiedRule::Yaml(r) => match r.severity {
                IssueSeverity::Critical => Severity::Critical,
                IssueSeverity::High => Severity::High,
                IssueSeverity::Medium => Severity::Medium,
                IssueSeverity::Low => Severity::Low,
                IssueSeverity::Info => Severity::Info,
            },
        }
    }

    pub fn category(&self) -> ThreatCategory {
        match self {
            UnifiedRule::Builtin(r) => r.category,
            UnifiedRule::Yaml(r) => r.category,
        }
    }

    pub fn weight(&self) -> i32 {
        match self {
            UnifiedRule::Builtin(r) => r.weight,
            UnifiedRule::Yaml(r) => r.weight,
        }
    }

    pub fn hard_trigger(&self) -> bool {
        match self {
            UnifiedRule::Builtin(r) => r.hard_trigger,
            UnifiedRule::Yaml(r) => r.hard_trigger,
        }
    }

    pub fn confidence(&self) -> Confidence {
        match self {
            UnifiedRule::Builtin(r) => r.confidence,
            UnifiedRule::Yaml(r) => r.confidence_enum(),
        }
    }

    pub fn description(&self) -> &str {
        match self {
            UnifiedRule::Builtin(r) => r.description,
            UnifiedRule::Yaml(r) => &r.description,
        }
    }

    pub fn remediation(&self) -> &str {
        match self {
            UnifiedRule::Builtin(r) => r.remediation,
            UnifiedRule::Yaml(r) => &r.remediation,
        }
    }

    pub fn cwe_id(&self) -> Option<&str> {
        match self {
            UnifiedRule::Builtin(r) => r.cwe_id,
            UnifiedRule::Yaml(r) => r.cwe_id.as_deref(),
        }
    }

    /// 获取适用的文件扩展名（None 表示匹配全部）
    pub fn file_types(&self) -> Option<&[String]> {
        match self {
            UnifiedRule::Builtin(_) => None, // 内置规则使用原有的 rule_applies_to_extension
            UnifiedRule::Yaml(r) => {
                if r.file_types.is_empty() {
                    None
                } else {
                    Some(&r.file_types)
                }
            }
        }
    }

    /// 获取抑制规则列表
    pub fn suppress_if_matched(&self) -> &[String] {
        match self {
            UnifiedRule::Builtin(_) => &[],
            UnifiedRule::Yaml(r) => &r.suppress_if_matched,
        }
    }

    /// 获取排除模式
    pub fn exclude_patterns(&self) -> &[String] {
        match self {
            UnifiedRule::Builtin(_) => &[],
            UnifiedRule::Yaml(r) => &r.exclude_patterns,
        }
    }
}
