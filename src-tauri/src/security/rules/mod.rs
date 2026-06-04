//! 安全规则体系
//!
//! - `types`: 共用类型（Severity, Category, Confidence）
//! - `loader`: YAML 规则包加载
//! - `pattern_engine`: 增强版规则匹配引擎

pub mod loader;
pub mod pattern_engine;
pub mod types;

// 从 types 重新导出
pub use types::{Category, Confidence};

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
