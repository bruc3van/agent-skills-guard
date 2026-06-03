//! 可分析性评估模块（Analyzability Assessment）
//!
//! 评估 Skill 目录的扫描覆盖率：统计可分析内容占比，
//! 识别不可分析的二进制文件、超大截断文件、以及文件数量/大小限制。
//!
//! - 可分析文件（Markdown/Script/Config/Text/Asset）100% 计入
//! - 已知惰性资产（.png, .jpg, .ttf 等）0% 计入但不产生 finding
//! - 未知二进制文件（is_binary=true 且非已知惰性类型）产生 UNANALYZABLE_BINARY finding
//! - 超大截断文件按实际扫描字节 / 总字节部分计入

use sha2::{Digest, Sha256};

use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use crate::security::skill_context::{SkillContext, SkillFileType};

// ── 常量 ──

const ANALYZER_NAME: &str = "analyzability";

/// 可分析性分数低于此阈值时产生 LOW_ANALYZABILITY finding
const LOW_ANALYZABILITY_THRESHOLD: f64 = 70.0;

// ── 公共接口 ──

/// 可分析性评估结果
#[derive(Debug, Clone)]
pub struct AnalyzabilityResult {
    /// 可分析性分数（0.0 - 100.0），可分析内容占比
    pub score: f64,
    /// 所有发现
    pub findings: Vec<Finding>,
    /// 总字节数
    pub total_bytes: u64,
    /// 可分析字节数
    pub analyzable_bytes: u64,
    /// 不可分析字节数
    pub unanalyzable_bytes: u64,
}

/// 评估目录的可分析性
pub fn assess(ctx: &SkillContext) -> AnalyzabilityResult {
    let mut total_bytes = 0u64;
    let mut analyzable_bytes = 0u64;
    let mut findings = Vec::new();

    for file in &ctx.files {
        total_bytes += file.size_bytes;

        if is_analyzable(file) {
            analyzable_bytes += file.size_bytes;
        } else if is_inert_asset(file) {
            // 低风险不可分析，不产生 finding
        } else if file.is_binary {
            // 高风险不可分析
            findings.push(make_unanalyzable_finding(file));
        }
        // 注意：非二进制且非可分析、非惰性的文件（如 Unknown 类型但非二进制）
        // 不计入可分析字节，也不产生 finding
    }

    // 超大文件检查
    let max_size = ctx.scan_policy.file_limits.max_scan_file_size_bytes;
    for file in &ctx.files {
        if file.size_bytes > max_size {
            findings.push(make_oversized_finding(file, max_size));
        }
    }

    // 文件数检查
    let max_files = ctx.scan_policy.file_limits.max_files;
    if ctx.files.len() > max_files {
        findings.push(make_excessive_file_count_finding(ctx));
    }

    // 可分析性分数
    let score = if total_bytes > 0 {
        (analyzable_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        100.0
    };

    // 低可分析性检查
    if score < LOW_ANALYZABILITY_THRESHOLD {
        findings.push(make_low_analyzability_finding(score));
    }

    AnalyzabilityResult {
        score,
        findings,
        total_bytes,
        analyzable_bytes,
        unanalyzable_bytes: total_bytes - analyzable_bytes,
    }
}

// ── 文件分类辅助函数 ──

/// 判断文件是否为可分析类型
///
/// 可分析类型：Markdown、Script、Config，以及文本形式的 Asset（.svg, .html, .css）
fn is_analyzable(file: &crate::security::skill_context::SkillFile) -> bool {
    match file.file_type {
        SkillFileType::Markdown | SkillFileType::Script | SkillFileType::Config => true,
        SkillFileType::Asset => is_text_asset(file),
        _ => false,
    }
}

/// 判断 Asset 文件是否为文本格式（如 .svg, .html, .css）
///
/// 文本资产可分析，二进制资产（如 .png, .jpg）为惰性资产
fn is_text_asset(file: &crate::security::skill_context::SkillFile) -> bool {
    let ext = file
        .absolute_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    matches!(
        ext.as_str(),
        "svg" | "html" | "htm" | "css" | "xml" | "xsd" | "xsl" | "xslt"
    )
}

/// 判断文件是否为已知惰性资产（低风险不可分析）
///
/// 惰性资产扩展名来自 policy 的 inert_extensions 列表
fn is_inert_asset(file: &crate::security::skill_context::SkillFile) -> bool {
    let ext = file
        .absolute_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    // 已知惰性扩展名（与 default_inert_extensions 一致）
    matches!(
        ext.as_str(),
        ".ttf" | ".otf" | ".woff" | ".woff2" | ".eot"
            | ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" | ".avif" | ".bmp"
            | ".ico" | ".icns" | ".tif" | ".tiff" | ".heic" | ".heif"
            | ".pyc" | ".pyo"
            | ".db" | ".sqlite" | ".sqlite3"
            // 其他已知二进制资产
            | ".mp3" | ".mp4" | ".wav" | ".ogg" | ".webm" | ".flac" | ".aac"
            | ".pdf" | ".doc" | ".docx" | ".xls" | ".xlsx" | ".ppt" | ".pptx"
            | ".zip" | ".gz" | ".tar" | ".rar" | ".7z" | ".bz2" | ".xz"
            | ".class" | ".o" | ".so" | ".dll" | ".dylib"
            | ".exe" | ".msi" | ".dmg"
            // 已知二进制类型
            | ".bin" | ".dat" | ".wasm" | ".jar" | ".war" | ".ear"
    )
}

// ── Finding 创建辅助函数 ──

/// 创建 Finding 实例
///
/// 使用 sha2 生成稳定的 finding ID：SHA256(rule_id + file)[:16]
fn make_finding(
    rule_id: &str,
    severity: IssueSeverity,
    title: &str,
    description: String,
    file_path: Option<String>,
) -> Finding {
    let id_input = format!(
        "{}|{}",
        rule_id,
        file_path.as_deref().unwrap_or(""),
    );
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let id = hash[..16].to_string();

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category: ThreatCategory::PolicyViolation,
        severity,
        title: title.to_string(),
        description,
        file_path,
        line_number: None,
        snippet: None,
        remediation: Some("Review file analyzability and adjust content as needed".to_string()),
        analyzer: ANALYZER_NAME.to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some("analyzability".to_string()),
            ..Default::default()
        }),
    }
}

/// 创建 UNANALYZABLE_BINARY finding
fn make_unanalyzable_finding(file: &crate::security::skill_context::SkillFile) -> Finding {
    let rel = file.relative_path.to_string_lossy().to_string();
    make_finding(
        "UNANALYZABLE_BINARY",
        IssueSeverity::Medium,
        "Unanalyzable binary file detected",
        format!(
            "File '{}' is binary and cannot be analyzed by security rules. \
             Consider removing or replacing with a known-safe alternative.",
            rel
        ),
        Some(rel),
    )
}

/// 创建 LOW_ANALYZABILITY finding
fn make_low_analyzability_finding(score: f64) -> Finding {
    make_finding(
        "LOW_ANALYZABILITY",
        IssueSeverity::Low,
        "Low analyzability score",
        format!(
            "Only {:.1}% of the skill content is analyzable. \
             This may indicate hidden or obfuscated content that cannot be scanned.",
            score
        ),
        None,
    )
}

/// 创建 EXCESSIVE_FILE_COUNT finding（带 type_breakdown）
fn make_excessive_file_count_finding(ctx: &SkillContext) -> Finding {
    let count = ctx.files.len();
    let max = ctx.scan_policy.file_limits.max_files;

    // 按 SkillFileType 统计文件数
    let mut type_breakdown = std::collections::HashMap::new();
    for file in &ctx.files {
        let type_name = match file.file_type {
            SkillFileType::Markdown => "markdown",
            SkillFileType::Script => "script",
            SkillFileType::Config => "config",
            SkillFileType::Asset => "asset",
            SkillFileType::Binary => "binary",
            SkillFileType::Unknown => "unknown",
        };
        *type_breakdown.entry(type_name.to_string()).or_insert(0) += 1;
    }

    let breakdown_str = type_breakdown
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect::<Vec<_>>()
        .join(", ");

    Finding {
        id: format!("analyzability:EXCESSIVE_FILE_COUNT:{}", count),
        rule_id: "EXCESSIVE_FILE_COUNT".to_string(),
        category: ThreatCategory::PolicyViolation,
        severity: IssueSeverity::Info,
        title: "Excessive file count".to_string(),
        description: format!(
            "Skill contains {} files, exceeding the policy limit of {}. \
             Some files may not be fully scanned. Breakdown: {}",
            count, max, breakdown_str
        ),
        file_path: None,
        line_number: None,
        snippet: None,
        remediation: Some("Review file analyzability and adjust content as needed".to_string()),
        analyzer: ANALYZER_NAME.to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some("analyzability".to_string()),
            ..Default::default()
        }),
    }
}

/// 创建 OVERSIZED_FILE finding
fn make_oversized_finding(
    file: &crate::security::skill_context::SkillFile,
    max_size: u64,
) -> Finding {
    let rel = file.relative_path.to_string_lossy().to_string();
    make_finding(
        "OVERSIZED_FILE",
        IssueSeverity::Info,
        "Oversized file detected",
        format!(
            "File '{}' is {} bytes, exceeding the scan limit of {} bytes. \
             Only the first {} bytes will be scanned.",
            rel,
            file.size_bytes,
            max_size,
            max_size,
        ),
        Some(rel),
    )
}

// ── 单元测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::policy::ScanPolicy;
    use crate::security::skill_context::{ScanMode, SkillContext, SkillFile, SkillFileType};
    use std::path::PathBuf;

    /// 创建测试用 SkillContext
    fn make_test_ctx(files: Vec<SkillFile>) -> SkillContext {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(
            ScanMode::Directory,
            Some(PathBuf::from("/tmp/test-skill")),
            policy,
        );
        ctx.files = files;
        ctx
    }

    /// 创建测试文件
    fn make_test_file(rel: &str, ext: &str, file_type: SkillFileType, is_binary: bool, size: u64) -> SkillFile {
        SkillFile {
            relative_path: PathBuf::from(rel),
            absolute_path: PathBuf::from(format!("/tmp/test-skill/{}", rel)),
            file_type,
            size_bytes: size,
            is_binary,
            is_hidden: false,
        }
    }

    #[test]
    fn test_all_analyzable_files_score_100() {
        let files = vec![
            make_test_file("skill.md", "md", SkillFileType::Markdown, false, 1000),
            make_test_file("run.sh", "sh", SkillFileType::Script, false, 500),
            make_test_file("config.json", "json", SkillFileType::Config, false, 200),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        assert!((result.score - 100.0).abs() < f64::EPSILON);
        assert!(result.findings.is_empty());
        assert_eq!(result.total_bytes, 1700);
        assert_eq!(result.analyzable_bytes, 1700);
        assert_eq!(result.unanalyzable_bytes, 0);
    }

    #[test]
    fn test_known_binary_png_no_unanalyzable_finding() {
        let files = vec![
            make_test_file("skill.md", "md", SkillFileType::Markdown, false, 1000),
            make_test_file("logo.png", "png", SkillFileType::Asset, true, 5000),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        // .png 是惰性资产，不产生 UNANALYZABLE_BINARY
        assert!(
            !result.findings.iter().any(|f| f.rule_id == "UNANALYZABLE_BINARY"),
            "Known inert .png should not produce UNANALYZABLE_BINARY finding"
        );
        // 分数：可分析 1000 / 总 6000 = 16.7%
        let expected_score = (1000.0 / 6000.0) * 100.0;
        assert!((result.score - expected_score).abs() < 0.01);
        // 应产生 LOW_ANALYZABILITY（分数 < 70%）
        assert!(
            result.findings.iter().any(|f| f.rule_id == "LOW_ANALYZABILITY"),
            "Should produce LOW_ANALYZABILITY when score < 70%"
        );
    }

    #[test]
    fn test_unknown_binary_produces_unanalyzable_finding() {
        let files = vec![
            make_test_file("skill.md", "md", SkillFileType::Markdown, false, 1000),
            make_test_file("mystery.xyz", "xyz", SkillFileType::Unknown, true, 2000),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        let unanalyzable: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "UNANALYZABLE_BINARY")
            .collect();
        assert_eq!(unanalyzable.len(), 1, "Should produce exactly one UNANALYZABLE_BINARY finding");
        assert_eq!(unanalyzable[0].file_path.as_deref(), Some("mystery.xyz"));
        assert!(matches!(unanalyzable[0].severity, IssueSeverity::Medium));
    }

    #[test]
    fn test_oversized_file_produces_finding() {
        let policy = ScanPolicy::builtin_default().clone();
        let max_size = policy.file_limits.max_scan_file_size_bytes;

        let files = vec![
            make_test_file(
                "huge_data.bin",
                "bin",
                SkillFileType::Binary,
                true,
                max_size + 1,
            ),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        assert!(
            result.findings.iter().any(|f| f.rule_id == "OVERSIZED_FILE"),
            "Should produce OVERSIZED_FILE for file exceeding scan limit"
        );
    }

    #[test]
    fn test_excessive_file_count_produces_finding() {
        let policy = ScanPolicy::builtin_default().clone();
        let max_files = policy.file_limits.max_files;

        let mut files: Vec<SkillFile> = (0..=max_files)
            .map(|i| {
                make_test_file(
                    &format!("file_{}.txt", i),
                    "txt",
                    SkillFileType::Config,
                    false,
                    10,
                )
            })
            .collect();
        // 加一个可分析文件确保不触发 LOW_ANALYZABILITY
        files.push(make_test_file("skill.md", "md", SkillFileType::Markdown, false, 1000));

        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        let excessive: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "EXCESSIVE_FILE_COUNT")
            .collect();
        assert_eq!(excessive.len(), 1, "Should produce exactly one EXCESSIVE_FILE_COUNT finding");
        assert!(matches!(excessive[0].severity, IssueSeverity::Info));
    }

    #[test]
    fn test_low_analyzability_produces_finding() {
        let files = vec![
            make_test_file("skill.md", "md", SkillFileType::Markdown, false, 100),
            make_test_file("big.bin", "bin", SkillFileType::Binary, true, 900),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        // 可分析分数 = 10%
        assert!(result.score < 70.0);
        assert!(
            result.findings.iter().any(|f| f.rule_id == "LOW_ANALYZABILITY"),
            "Should produce LOW_ANALYZABILITY when score < 70%"
        );
    }

    #[test]
    fn test_empty_directory_score_100() {
        let ctx = make_test_ctx(Vec::new());
        let result = assess(&ctx);

        assert!((result.score - 100.0).abs() < f64::EPSILON);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn test_text_asset_is_analyzable() {
        let svg = make_test_file("icon.svg", "svg", SkillFileType::Asset, false, 500);
        assert!(is_analyzable(&svg), ".svg should be analyzable");

        let html = make_test_file("page.html", "html", SkillFileType::Asset, false, 500);
        assert!(is_analyzable(&html), ".html should be analyzable");

        let css = make_test_file("style.css", "css", SkillFileType::Asset, false, 500);
        assert!(is_analyzable(&css), ".css should be analyzable");
    }

    #[test]
    fn test_binary_asset_is_not_analyzable_but_inert() {
        let png = make_test_file("image.png", "png", SkillFileType::Asset, true, 5000);
        assert!(!is_analyzable(&png), ".png should not be analyzable");
        assert!(is_inert_asset(&png), ".png should be inert");

        let ttf = make_test_file("font.ttf", "ttf", SkillFileType::Asset, true, 50000);
        assert!(!is_analyzable(&ttf), ".ttf should not be analyzable");
        assert!(is_inert_asset(&ttf), ".ttf should be inert");
    }

    #[test]
    fn test_known_binary_ext_is_inert() {
        let bin = make_test_file("data.bin", "bin", SkillFileType::Binary, true, 1000);
        assert!(is_inert_asset(&bin), ".bin should be inert");

        let wasm = make_test_file("module.wasm", "wasm", SkillFileType::Binary, true, 1000);
        assert!(is_inert_asset(&wasm), ".wasm should be inert");

        let exe = make_test_file("app.exe", "exe", SkillFileType::Asset, true, 1000);
        assert!(is_inert_asset(&exe), ".exe should be inert");
    }

    #[test]
    fn test_unknown_ext_with_is_binary_not_inert() {
        let mystery = make_test_file("data.xyz", "xyz", SkillFileType::Unknown, true, 1000);
        assert!(!is_inert_asset(&mystery), ".xyz should not be inert");
    }

    #[test]
    fn test_finding_analyzer_is_set() {
        let files = vec![
            make_test_file("mystery.xyz", "xyz", SkillFileType::Unknown, true, 1000),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        for finding in &result.findings {
            assert_eq!(finding.analyzer, "analyzability");
        }
    }

    #[test]
    fn test_finding_has_stable_id() {
        let file = make_test_file("mystery.xyz", "xyz", SkillFileType::Unknown, true, 1000);
        let f1 = make_unanalyzable_finding(&file);
        let f2 = make_unanalyzable_finding(&file);
        assert_eq!(f1.id, f2.id, "Same inputs should produce same ID");
    }

    #[test]
    fn test_oversized_and_unknown_binary_combined() {
        let policy = ScanPolicy::builtin_default().clone();
        let max_size = policy.file_limits.max_scan_file_size_bytes;

        let files = vec![
            make_test_file("skill.md", "md", SkillFileType::Markdown, false, 1000),
            make_test_file(
                "huge_unknown.xyz",
                "xyz",
                SkillFileType::Unknown,
                true,
                max_size + 100,
            ),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        // 应同时产生 UNANALYZABLE_BINARY 和 OVERSIZED_FILE
        assert!(
            result.findings.iter().any(|f| f.rule_id == "UNANALYZABLE_BINARY"),
            "Should detect unanalyzable binary"
        );
        assert!(
            result.findings.iter().any(|f| f.rule_id == "OVERSIZED_FILE"),
            "Should detect oversized file"
        );
    }

    #[test]
    fn test_multiple_unknown_binaries() {
        let files = vec![
            make_test_file("a.xyz", "xyz", SkillFileType::Unknown, true, 500),
            make_test_file("b.abc", "abc", SkillFileType::Unknown, true, 500),
            make_test_file("skill.md", "md", SkillFileType::Markdown, false, 1000),
        ];
        let ctx = make_test_ctx(files);
        let result = assess(&ctx);

        let unanalyzable: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "UNANALYZABLE_BINARY")
            .collect();
        assert_eq!(unanalyzable.len(), 2, "Should produce one finding per unknown binary");
    }
}
