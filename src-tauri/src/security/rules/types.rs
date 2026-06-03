//! 安全规则共用类型
//!
//! 从 builtin_compat.rs 提取的活跃类型，供 scanner 和 pattern_engine 使用。

use crate::models::security::{IssueSeverity, ThreatCategory};
use serde::{Deserialize, Serialize};

/// 风险严重程度（5 级统一模型，与 IssueSeverity 一一对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl From<Severity> for IssueSeverity {
    fn from(s: Severity) -> Self {
        match s {
            Severity::Critical => IssueSeverity::Critical,
            Severity::High => IssueSeverity::High,
            Severity::Medium => IssueSeverity::Medium,
            Severity::Low => IssueSeverity::Low,
            Severity::Info => IssueSeverity::Info,
        }
    }
}

/// 风险类别（统一使用 ThreatCategory，保持向后兼容的别名）
pub type Category = ThreatCategory;

/// 置信度等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    High,   // 高置信度，误报可能性低
    Medium, // 中等置信度
    Low,    // 低置信度，可能误报
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::High => "High",
            Confidence::Medium => "Medium",
            Confidence::Low => "Low",
        }
    }

    /// 评分扣分系数（硬触发规则不使用此系数，见 SecurityScanner::effective_rule_weight）
    pub fn score_multiplier(&self) -> f32 {
        match self {
            Confidence::High => 1.0,
            Confidence::Medium => 0.65,
            Confidence::Low => 0.35,
        }
    }
}
