//! 共享 Finding 构建器
//!
//! 提供统一的 Finding ID 生成和构造函数，供所有分析器模块共用。
//! 各模块只需提供业务参数，ID 生成、FindingMetadata 组装等细节在此统一处理。

use sha2::{Digest, Sha256};

use crate::models::security::{
    Finding, FindingKind, FindingMetadata, IssueSeverity, ThreatCategory,
};

/// Finding 规格参数（各分析器提供）
pub struct FindingSpec<'a> {
    pub rule_id: &'a str,
    pub category: ThreatCategory,
    pub severity: IssueSeverity,
    pub title: &'a str,
    pub description: String,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub snippet: Option<String>,
    pub remediation: Option<String>,
    pub analyzer: &'a str,
    pub finding_kind: FindingKind,
    // ── 可选元数据（默认 None） ──
    /// 覆盖 metadata.rule_source（默认等于 analyzer）
    pub rule_source: Option<&'a str>,
    /// CWE 标识（如 "CWE-434"）
    pub cwe_id: Option<String>,
    /// 置信度（如 "High"、"Medium"、"Low"）
    pub confidence: Option<String>,
    /// ID 盐值：参与哈希计算的额外数据，用于确保同一 rule_id + file_path + line_number
    /// 下多个 findings 的唯一性（例如 cross_skill 用 description 区分不同发现）
    pub id_salt: Option<&'a str>,
}

/// 生成稳定的 Finding ID
///
/// 使用 SHA256(rule_id|file_path|line_number|salt)[:16] 作为 ID，
/// 确保相同位置的相同规则产生相同的 ID（幂等去重）。
pub fn make_finding_id(
    rule_id: &str,
    file_path: &str,
    line_number: Option<usize>,
    salt: Option<&str>,
) -> String {
    let id_input = format!(
        "{}|{}|{}{}",
        rule_id,
        file_path,
        line_number.map(|l| l.to_string()).unwrap_or_default(),
        salt.unwrap_or("")
    );
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    hash[..16].to_string()
}

/// 根据规格参数构造 Finding
///
/// 统一处理：
/// - ID 生成（SHA256 hash）
/// - FindingMetadata 的 rule_source、finding_kind、cwe_id、confidence
/// - 默认 remediation
pub fn make_finding(spec: FindingSpec) -> Finding {
    let id = make_finding_id(
        spec.rule_id,
        spec.file_path.as_deref().unwrap_or(""),
        spec.line_number,
        spec.id_salt,
    );

    Finding {
        id,
        rule_id: spec.rule_id.to_string(),
        category: spec.category,
        severity: spec.severity,
        title: spec.title.to_string(),
        description: spec.description,
        file_path: spec.file_path,
        line_number: spec.line_number,
        snippet: spec.snippet,
        remediation: spec.remediation,
        analyzer: spec.analyzer.to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some(
                spec.rule_source
                    .unwrap_or(spec.analyzer)
                    .to_string(),
            ),
            finding_kind: Some(spec.finding_kind),
            cwe_id: spec.cwe_id,
            confidence: spec.confidence,
            ..Default::default()
        }),
    }
}
