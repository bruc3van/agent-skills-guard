//! Archive 提取器
//!
//! 安全地提取压缩包（ZIP、Office OOXML 等），执行安全检查：
//! - ZIP bomb 检测（压缩比超限）
//! - 路径穿越检测（`..` 条目）
//! - 嵌套深度超限
//! - 文件数 / 总大小超限
//! - Office VBA 宏 / OLE 嵌入检测
//!
//! 提取的文件写入临时目录，供后续扫描流水线消费。

use std::fs;
use std::io;
use std::path::Path;

use tempfile::TempDir;
use zip::ZipArchive;

use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use crate::security::policy::ScanPolicy;

// ── 公共类型 ──

/// 归档提取结果
pub struct ExtractionResult {
    /// 临时目录（调用方负责管理生命周期）
    pub temp_dir: Option<TempDir>,
    /// 提取的文件绝对路径列表
    pub extracted_files: Vec<String>,
    /// 安全发现
    pub findings: Vec<Finding>,
}

/// 支持的归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveType {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    OfficeDocx,
    OfficeXlsx,
    OfficePptx,
}

impl ArchiveType {
    /// 从文件扩展名解析
    pub fn from_path(path: &str) -> Option<Self> {
        let lower = path.to_lowercase();
        let ext = Path::new(&lower)
            .extension()
            .and_then(|e| e.to_str())?;

        match ext {
            "zip" => Some(Self::Zip),
            "tar" => Some(Self::Tar),
            "gz" => {
                // 检查是否为 .tar.gz
                let stem = Path::new(&lower)
                    .file_stem()
                    .and_then(|s| s.to_str())?;
                if stem.ends_with(".tar") {
                    Some(Self::TarGz)
                } else {
                    Some(Self::TarGz) // 单独的 .gz 也归为 TarGz 处理
                }
            }
            "bz2" => Some(Self::TarBz2),
            "xz" => Some(Self::TarXz),
            "docx" => Some(Self::OfficeDocx),
            "xlsx" => Some(Self::OfficeXlsx),
            "pptx" => Some(Self::OfficePptx),
            _ => None,
        }
    }

    /// 是否为 Office 文档（基于 OOXML/ZIP）
    pub fn is_office(&self) -> bool {
        matches!(
            self,
            Self::OfficeDocx | Self::OfficeXlsx | Self::OfficePptx
        )
    }

    /// 是否为 ZIP 格式（含 Office）
    pub fn is_zip_based(&self) -> bool {
        matches!(
            self,
            Self::Zip | Self::OfficeDocx | Self::OfficeXlsx | Self::OfficePptx
        )
    }
}

// ── 公共 API ──

/// 根据扩展名检测归档类型
pub fn detect_archive_type(path: &str) -> Option<ArchiveType> {
    ArchiveType::from_path(path)
}

/// 提取归档文件到临时目录
pub fn extract_archive(archive_path: &str, policy: &ScanPolicy) -> ExtractionResult {
    let archive_type = match detect_archive_type(archive_path) {
        Some(t) => t,
        None => {
            return ExtractionResult {
                temp_dir: None,
                extracted_files: Vec::new(),
                findings: vec![make_finding(
                    "ARCHIVE_UNSUPPORTED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Unsupported archive format",
                    &format!("Cannot extract '{}': unsupported archive type", archive_path),
                    Some(archive_path.to_string()),
                )],
            }
        }
    };

    if !archive_type.is_zip_based() {
        // TAR 系列暂时标记为 TODO
        return ExtractionResult {
            temp_dir: None,
            extracted_files: Vec::new(),
            findings: vec![make_finding(
                "ARCHIVE_UNSUPPORTED",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Archive format not yet supported for extraction",
                &format!(
                    "'{}' is a {:?} archive; extraction not yet implemented",
                    archive_path, archive_type
                ),
                Some(archive_path.to_string()),
            )],
        };
    }

    extract_zip(archive_path, policy, &archive_type)
}

// ── ZIP 提取逻辑 ──

fn extract_zip(
    archive_path: &str,
    policy: &ScanPolicy,
    archive_type: &ArchiveType,
) -> ExtractionResult {
    let mut findings = Vec::new();
    let mut extracted_files = Vec::new();

    let file = match fs::File::open(archive_path) {
        Ok(f) => f,
        Err(e) => {
            findings.push(make_finding(
                "ARCHIVE_EXTRACTION_FAILED",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Failed to open archive file",
                &format!("Cannot open '{}': {}", archive_path, e),
                Some(archive_path.to_string()),
            ));
            return ExtractionResult {
                temp_dir: None,
                extracted_files,
                findings,
            };
        }
    };

    let mut archive = match ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            findings.push(make_finding(
                "ARCHIVE_EXTRACTION_FAILED",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Failed to read archive (invalid or corrupted)",
                &format!("Cannot read '{}': {}", archive_path, e),
                Some(archive_path.to_string()),
            ));
            return ExtractionResult {
                temp_dir: None,
                extracted_files,
                findings,
            };
        }
    };

    let temp_dir = match TempDir::new() {
        Ok(d) => d,
        Err(e) => {
            findings.push(make_finding(
                "ARCHIVE_EXTRACTION_FAILED",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Failed to create temporary directory",
                &format!("TempDir error: {}", e),
                Some(archive_path.to_string()),
            ));
            return ExtractionResult {
                temp_dir: None,
                extracted_files,
                findings,
            };
        }
    };

    let mut total_size: u64 = 0;
    let mut file_count: usize = 0;
    let mut nested_archives: Vec<String> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_name = entry.name().to_string();

        // ── 路径穿越检查 ──
        if entry_name.contains("..") {
            findings.push(make_finding(
                "ARCHIVE_PATH_TRAVERSAL",
                IssueSeverity::Critical,
                ThreatCategory::SensitiveFileAccess,
                "Path traversal detected in archive entry",
                &format!(
                    "Archive entry '{}' contains '..' which may escape the extraction directory",
                    entry_name
                ),
                Some(entry_name.clone()),
            ));
            continue; // 跳过此条目，不解压
        }

        // ── ZIP bomb 检查（压缩比） ──
        let compressed = entry.compressed_size();
        let uncompressed = entry.size();
        if compressed > 0 {
            let ratio = uncompressed as f64 / compressed as f64;
            if ratio > policy.archive.max_compression_ratio {
                findings.push(make_finding(
                    "ARCHIVE_ZIP_BOMB",
                    IssueSeverity::High,
                    ThreatCategory::Destructive,
                    "Potential ZIP bomb detected",
                    &format!(
                        "Entry '{}' has compression ratio {:.1}:1 (threshold: {:.1}:1), \
                         compressed {} bytes -> uncompressed {} bytes",
                        entry_name, ratio, policy.archive.max_compression_ratio,
                        compressed, uncompressed
                    ),
                    Some(entry_name.clone()),
                ));
                continue;
            }
        }

        // ── 文件数限制 ──
        file_count += 1;
        if file_count > policy.archive.max_file_count {
            findings.push(make_finding(
                "ARCHIVE_TOO_MANY_FILES",
                IssueSeverity::Low,
                ThreatCategory::Obfuscation,
                "Archive contains too many files",
                &format!(
                    "Archive exceeds file count limit: {} files (limit: {})",
                    file_count, policy.archive.max_file_count
                ),
                Some(archive_path.to_string()),
            ));
            break;
        }

        // ── 总大小限制 ──
        total_size += uncompressed;
        if total_size > policy.archive.max_total_size_bytes {
            findings.push(make_finding(
                "ARCHIVE_TOO_LARGE",
                IssueSeverity::Low,
                ThreatCategory::Obfuscation,
                "Archive uncompressed size exceeds limit",
                &format!(
                    "Archive total uncompressed size {} bytes exceeds limit of {} bytes",
                    total_size, policy.archive.max_total_size_bytes
                ),
                Some(archive_path.to_string()),
            ));
            break;
        }

        // ── Office VBA / OLE 检查 ──
        if archive_type.is_office() {
            check_office_threats(&entry_name, &mut findings);
        }

        // ── 记录嵌套归档 ──
        if is_archive_extension(&entry_name) {
            nested_archives.push(entry_name.clone());
        }

        // ── 跳过目录条目 ──
        if entry.is_dir() {
            // 确保目录存在
            let out_path = temp_dir.path().join(entry.name());
            let _ = fs::create_dir_all(&out_path);
            continue;
        }

        // ── 解压文件 ──
        let out_path = temp_dir.path().join(entry.name());

        // 防止创建意外的父目录（条目名含 / 的情况）
        if let Some(parent) = out_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to create directory for archive entry",
                    &format!("Cannot create directory '{}': {}", parent.display(), e),
                    Some(entry_name.clone()),
                ));
                continue;
            }
        }

        match fs::File::create(&out_path) {
            Ok(mut out_file) => {
                if let Err(e) = io::copy(&mut entry, &mut out_file) {
                    findings.push(make_finding(
                        "ARCHIVE_EXTRACTION_FAILED",
                        IssueSeverity::Medium,
                        ThreatCategory::Obfuscation,
                        "Failed to extract archive entry",
                        &format!(
                            "Cannot extract '{}': {}",
                            entry_name, e
                        ),
                        Some(entry_name.clone()),
                    ));
                    continue;
                }
            }
            Err(e) => {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to create output file for archive entry",
                    &format!(
                        "Cannot create file '{}': {}",
                        out_path.display(), e
                    ),
                    Some(entry_name.clone()),
                ));
                continue;
            }
        }

        extracted_files.push(out_path.to_string_lossy().to_string());
    }

    // ── 嵌套深度检查 ──
    let max_depth = calculate_nested_depth(&nested_archives);
    if max_depth > policy.archive.max_depth {
        findings.push(make_finding(
            "ARCHIVE_NESTED_TOO_DEEP",
            IssueSeverity::Medium,
            ThreatCategory::Obfuscation,
            "Archive contains nested archives exceeding depth limit",
            &format!(
                "Detected {} levels of nested archives (limit: {})",
                max_depth, policy.archive.max_depth
            ),
            Some(archive_path.to_string()),
        ));
    }

    ExtractionResult {
        temp_dir: Some(temp_dir),
        extracted_files,
        findings,
    }
}

// ── Office 威胁检查 ──

/// 检查 Office 文档 ZIP 条目中的 VBA 宏和 OLE 嵌入
fn check_office_threats(entry_name: &str, findings: &mut Vec<Finding>) {
    let lower = entry_name.to_lowercase();

    // VBA 宏检测
    if lower.contains("vbaproject") {
        findings.push(make_finding(
            "OFFICE_VBA_MACRO",
            IssueSeverity::Critical,
            ThreatCategory::Destructive,
            "Office document contains VBA macro",
            &format!(
                "Office archive entry '{}' appears to contain VBA macro code (vbaProject)",
                entry_name
            ),
            Some(entry_name.to_string()),
        ));
    }

    // OLE 嵌入检测
    if lower.contains("oleobject") || lower.contains("embeddings") {
        findings.push(make_finding(
            "OFFICE_EMBEDDED_OLE",
            IssueSeverity::High,
            ThreatCategory::Obfuscation,
            "Office document contains embedded OLE object",
            &format!(
                "Office archive entry '{}' appears to contain an embedded OLE object",
                entry_name
            ),
            Some(entry_name.to_string()),
        ));
    }
}

// ── 辅助函数 ──

/// 判断文件名是否为归档类型扩展名
fn is_archive_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    let archive_exts = ["zip", "tar", "gz", "bz2", "xz", "rar", "7z"];
    archive_exts.iter().any(|ext| lower.ends_with(&format!(".{}", ext)))
}

/// 计算嵌套归档的最大深度
/// 简单策略：统计嵌套归档列表中同时出现在其他归档内部的条目数量
fn calculate_nested_depth(nested_archives: &[String]) -> usize {
    // 如果存在至少一个嵌套归档，视为深度 1
    // 更精确的深度检测需要递归扫描嵌套归档，此处简化
    if nested_archives.is_empty() {
        0
    } else {
        // 找出那些路径前缀出现在其他条目中的条目（即被嵌套在另一层归档中）
        let mut depth = 1;
        for (i, outer) in nested_archives.iter().enumerate() {
            for (j, inner) in nested_archives.iter().enumerate() {
                if i != j && inner.starts_with(outer.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '.')) {
                    depth = depth.max(2);
                }
            }
        }
        depth
    }
}

/// 创建统一的 Finding
fn make_finding(
    rule_id: &str,
    severity: IssueSeverity,
    category: ThreatCategory,
    title: &str,
    description: &str,
    file_path: Option<String>,
) -> Finding {
    Finding {
        id: format!("archive_extractor:{}", rule_id),
        rule_id: rule_id.to_string(),
        category,
        severity,
        title: title.to_string(),
        description: description.to_string(),
        file_path,
        line_number: None,
        snippet: None,
        remediation: Some(remediation_for_rule(rule_id).to_string()),
        analyzer: "archive_extractor".to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some("archive_extractor".to_string()),
            ..Default::default()
        }),
    }
}

/// 为每条规则提供修复建议
fn remediation_for_rule(rule_id: &str) -> &'static str {
    match rule_id {
        "ARCHIVE_ZIP_BOMB" => {
            "Reject archive with suspicious compression ratio. \
             Verify the archive is from a trusted source."
        }
        "ARCHIVE_PATH_TRAVERSAL" => {
            "Reject archive entries with '..' in paths. \
             These may write files outside the intended directory."
        }
        "ARCHIVE_NESTED_TOO_DEEP" => {
            "Flatten nested archives before extraction. \
             Deep nesting may be used to evade detection."
        }
        "ARCHIVE_EXTRACTION_FAILED" => {
            "Verify the archive is not corrupted or truncated. \
             Check file permissions."
        }
        "ARCHIVE_TOO_MANY_FILES" => {
            "Split large archives into smaller bundles. \
             Excessive file count may indicate a resource exhaustion attack."
        }
        "ARCHIVE_TOO_LARGE" => {
            "Split large archives into smaller bundles. \
             Uncompressed size exceeds the configured limit."
        }
        "OFFICE_VBA_MACRO" => {
            "Office document contains VBA macros. \
             Remove macros or scan them separately. \
             Macros can execute arbitrary code on open."
        }
        "OFFICE_EMBEDDED_OLE" => {
            "Office document contains embedded OLE objects. \
             Embedded objects may execute arbitrary code."
        }
        "ARCHIVE_UNSUPPORTED" => {
            "The archive format is not supported for extraction. \
             Convert to a supported format (ZIP) or scan the archive as a binary."
        }
        _ => "Review the archive contents manually.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn default_policy() -> ScanPolicy {
        ScanPolicy::builtin_default().clone()
    }

    /// 创建一个包含指定条目的 ZIP 文件，返回临时文件路径（.zip 后缀）
    fn create_test_zip(entries: Vec<(&str, &[u8])>) -> tempfile::NamedTempFile {
        let tmp = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .unwrap();
        let file = fs::File::create(tmp.path()).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, data) in entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(data).unwrap();
        }

        zip.finish().unwrap();
        tmp
    }

    #[test]
    fn test_detect_archive_type() {
        assert_eq!(detect_archive_type("file.zip"), Some(ArchiveType::Zip));
        assert_eq!(detect_archive_type("file.tar"), Some(ArchiveType::Tar));
        assert_eq!(detect_archive_type("file.tar.gz"), Some(ArchiveType::TarGz));
        assert_eq!(detect_archive_type("file.docx"), Some(ArchiveType::OfficeDocx));
        assert_eq!(detect_archive_type("file.xlsx"), Some(ArchiveType::OfficeXlsx));
        assert_eq!(detect_archive_type("file.pptx"), Some(ArchiveType::OfficePptx));
        assert_eq!(detect_archive_type("file.txt"), None);
        assert_eq!(detect_archive_type("file.xyz"), None);
    }

    #[test]
    fn test_extract_zip_normal() {
        let entries = vec![
            ("hello.txt", b"Hello, World!" as &[u8]),
            ("dir/nested.txt", b"Nested content" as &[u8]),
        ];
        let tmp = create_test_zip(entries);
        let policy = default_policy();

        let result = extract_archive(tmp.path().to_str().unwrap(), &policy);

        assert!(result.temp_dir.is_some(), "Should have a temp dir");
        assert_eq!(result.extracted_files.len(), 2, "Should extract 2 files");
        // 不应有 critical findings
        assert!(
            result.findings.iter().all(|f| f.severity != IssueSeverity::Critical),
            "No critical findings expected for normal ZIP"
        );
    }

    #[test]
    fn test_extract_zip_path_traversal() {
        // 路径穿越：条目名包含 ..
        let entries = vec![
            ("../../etc/passwd", b"root:x:0:0:root:/root:/bin/bash" as &[u8]),
            ("safe.txt", b"safe content" as &[u8]),
        ];
        let tmp = create_test_zip(entries);
        let policy = default_policy();

        let result = extract_archive(tmp.path().to_str().unwrap(), &policy);

        // 应检出路径穿越
        let traversal_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "ARCHIVE_PATH_TRAVERSAL")
            .collect();
        assert_eq!(traversal_findings.len(), 1, "Should detect path traversal");
        assert_eq!(traversal_findings[0].severity, IssueSeverity::Critical);

        // 路径穿越条目不应被解压
        let has_passwd = result
            .extracted_files
            .iter()
            .any(|f| f.contains("passwd"));
        assert!(!has_passwd, "Path traversal entry should not be extracted");

        // safe.txt 应被正常解压
        let has_safe = result
            .extracted_files
            .iter()
            .any(|f| f.contains("safe.txt"));
        assert!(has_safe, "Safe file should still be extracted");
    }

    #[test]
    fn test_extract_zip_bomb() {
        // 创建一个压缩比极高的 ZIP
        // 我们用 Deflate 模式并写入大量重复数据
        let tmp = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .unwrap();
        let file = fs::File::create(tmp.path()).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("bomb.txt", options).unwrap();
        // 写入 10MB 重复数据，Deflate 压缩后会很小
        let payload = vec![b'A'; 10 * 1024 * 1024];
        zip.write_all(&payload).unwrap();
        zip.finish().unwrap();

        let mut policy = default_policy();
        policy.archive.max_compression_ratio = 10.0; // 设低阈值以触发检测

        let result = extract_archive(tmp.path().to_str().unwrap(), &policy);

        let bomb_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "ARCHIVE_ZIP_BOMB")
            .collect();
        assert_eq!(bomb_findings.len(), 1, "Should detect ZIP bomb");
        assert_eq!(bomb_findings[0].severity, IssueSeverity::High);
    }

    #[test]
    fn test_extract_office_vba_macro() {
        // 模拟含 vbaProject.bin 的 Office 文档
        let entries = vec![
            ("[Content_Types].xml", b"xml content" as &[u8]),
            ("word/vbaProject.bin", b"\x00\x01\x02\x03" as &[u8]),
            ("word/document.xml", b"doc content" as &[u8]),
        ];
        let tmp = create_test_zip(entries);
        let mut policy = default_policy();
        // 设置较低的压缩比阈值以确保不会先触发 zip bomb
        policy.archive.max_compression_ratio = 1000.0;

        // 以 Office 文件名重命名
        let office_path = tmp.path().with_extension("docx");
        fs::copy(tmp.path(), &office_path).unwrap();

        let result = extract_archive(office_path.to_str().unwrap(), &policy);

        let vba_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "OFFICE_VBA_MACRO")
            .collect();
        assert_eq!(vba_findings.len(), 1, "Should detect VBA macro");
        assert_eq!(vba_findings[0].severity, IssueSeverity::Critical);

        fs::remove_file(&office_path).ok();
    }

    #[test]
    fn test_extract_file_count_limit() {
        let mut entries: Vec<(&str, &[u8])> = Vec::new();
        let data = b"data";
        for i in 0..10 {
            let name = format!("file_{}.txt", i);
            // Leak the string to get a &'static str
            let name_leak = Box::leak(name.into_boxed_str());
            entries.push((name_leak, data.as_ref()));
        }
        let tmp = create_test_zip(entries);

        let mut policy = default_policy();
        policy.archive.max_file_count = 5;

        let result = extract_archive(tmp.path().to_str().unwrap(), &policy);

        let count_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "ARCHIVE_TOO_MANY_FILES")
            .collect();
        assert_eq!(count_findings.len(), 1, "Should detect too many files");
        assert_eq!(count_findings[0].severity, IssueSeverity::Low);
    }

    #[test]
    fn test_extract_size_limit() {
        let big_data = vec![0u8; 1024];
        let entries = vec![
            ("big.bin", big_data.as_slice()),
            ("small.txt", b"tiny" as &[u8]),
        ];
        let tmp = create_test_zip(entries);

        let mut policy = default_policy();
        policy.archive.max_total_size_bytes = 500; // 500 bytes limit

        let result = extract_archive(tmp.path().to_str().unwrap(), &policy);

        let size_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "ARCHIVE_TOO_LARGE")
            .collect();
        assert_eq!(size_findings.len(), 1, "Should detect archive too large");
        assert_eq!(size_findings[0].severity, IssueSeverity::Low);
    }

    #[test]
    fn test_extract_office_ole_embedded() {
        let entries = vec![
            ("[Content_Types].xml", b"xml" as &[u8]),
            ("word/embeddings/oleObject1.bin", b"\x00\x01" as &[u8]),
            ("word/document.xml", b"doc" as &[u8]),
        ];
        let tmp = create_test_zip(entries);
        let mut policy = default_policy();
        policy.archive.max_compression_ratio = 1000.0;

        let office_path = tmp.path().with_extension("xlsx");
        fs::copy(tmp.path(), &office_path).unwrap();

        let result = extract_archive(office_path.to_str().unwrap(), &policy);

        let ole_findings: Vec<_> = result
            .findings
            .iter()
            .filter(|f| f.rule_id == "OFFICE_EMBEDDED_OLE")
            .collect();
        assert_eq!(ole_findings.len(), 1, "Should detect OLE embedded");
        assert_eq!(ole_findings[0].severity, IssueSeverity::High);

        fs::remove_file(&office_path).ok();
    }

    #[test]
    fn test_extract_unsupported_format() {
        let result = extract_archive("/tmp/file.rar", &default_policy());
        assert!(
            result.findings.iter().any(|f| f.rule_id == "ARCHIVE_UNSUPPORTED"),
            "Should report unsupported format"
        );
    }

    #[test]
    fn test_extract_nonexistent_file() {
        let result = extract_archive("/tmp/nonexistent_12345.zip", &default_policy());
        assert!(
            result.findings.iter().any(|f| f.rule_id == "ARCHIVE_EXTRACTION_FAILED"),
            "Should report extraction failure for missing file"
        );
    }

    #[test]
    fn test_extract_corrupted_zip() {
        let tmp = tempfile::Builder::new()
            .suffix(".zip")
            .tempfile()
            .unwrap();
        fs::write(tmp.path(), b"this is not a zip file").unwrap();

        let result = extract_archive(tmp.path().to_str().unwrap(), &default_policy());
        assert!(
            result.findings.iter().any(|f| f.rule_id == "ARCHIVE_EXTRACTION_FAILED"),
            "Should report extraction failure for corrupted ZIP"
        );
    }

    #[test]
    fn test_archive_type_properties() {
        assert!(ArchiveType::OfficeDocx.is_office());
        assert!(ArchiveType::OfficeXlsx.is_office());
        assert!(ArchiveType::OfficePptx.is_office());
        assert!(!ArchiveType::Zip.is_office());
        assert!(!ArchiveType::Tar.is_office());

        assert!(ArchiveType::Zip.is_zip_based());
        assert!(ArchiveType::OfficeDocx.is_zip_based());
        assert!(!ArchiveType::Tar.is_zip_based());
        assert!(!ArchiveType::TarGz.is_zip_based());
    }

    #[test]
    fn test_extract_zip_empty() {
        let entries: Vec<(&str, &[u8])> = Vec::new();
        let tmp = create_test_zip(entries);
        let result = extract_archive(tmp.path().to_str().unwrap(), &default_policy());

        assert!(result.temp_dir.is_some());
        assert_eq!(result.extracted_files.len(), 0);
        assert!(
            result.findings.is_empty(),
            "Empty ZIP should produce no findings"
        );
    }
}
