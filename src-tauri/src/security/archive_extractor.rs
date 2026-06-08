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

use crate::models::security::{Finding, FindingKind, IssueSeverity, ThreatCategory};
use crate::security::finding_builder::{self, FindingSpec};
use crate::security::policy::ScanPolicy;

// ── 安全路径校验 ──

/// 检查路径是否安全（在目标目录内）
///
/// 防护措施：
/// 1. 拒绝绝对路径（Unix 以 `/` 开头，Windows 以 `\\` 或盘符开头）
/// 2. 拒绝包含 `..` 的路径
/// 3. 规范化后验证最终路径确实在目标目录内
fn is_path_safe(entry_name: &str, target_dir: &Path) -> bool {
    let entry_path = Path::new(entry_name);

    // 拒绝绝对路径
    if entry_path.is_absolute() {
        return false;
    }

    // 检查路径组件中是否有 ..
    for component in entry_path.components() {
        match component {
            std::path::Component::ParentDir => return false,
            std::path::Component::RootDir => return false,
            _ => {}
        }
    }

    // 构建目标路径并规范化检查
    let out_path = target_dir.join(entry_name);

    // 尝试规范化路径（如果文件已存在）
    // 对于新创建的文件，我们检查其父目录
    if let Some(parent) = out_path.parent() {
        // 确保父目录存在以便 canonicalize
        let _ = fs::create_dir_all(parent);
    }

    // 使用 canonicalize 验证路径
    // 注意：对于尚不存在的文件，canonicalize 会失败
    // 所以我们检查规范化后的路径前缀
    match out_path.canonicalize() {
        Ok(canonical) => {
            let target_canonical = target_dir
                .canonicalize()
                .unwrap_or_else(|_| target_dir.to_path_buf());
            canonical.starts_with(&target_canonical)
        }
        Err(_) => {
            // 如果无法规范化（文件不存在），检查路径组件
            // 确保没有路径穿越
            let mut depth: i32 = 0;
            for component in entry_path.components() {
                match component {
                    std::path::Component::Normal(_) => depth += 1,
                    std::path::Component::ParentDir => depth -= 1,
                    _ => {}
                }
                if depth < 0 {
                    return false;
                }
            }
            true
        }
    }
}

/// 检查 TAR 条目是否为 symlink
fn is_tar_symlink(entry: &tar::Entry<'_, impl io::Read>) -> bool {
    // 检查 entry_type
    let entry_type = entry.header().entry_type();
    if entry_type.is_symlink() || entry_type == tar::EntryType::Symlink {
        return true;
    }

    // 在某些 TAR 实现中，symlink 可能被标记为 Regular
    // 但 link_name 会指向目标
    if let Ok(link_name) = entry.link_name() {
        if link_name.is_some() {
            return true;
        }
    }

    false
}

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
        let ext = Path::new(&lower).extension().and_then(|e| e.to_str())?;

        match ext {
            "zip" => Some(Self::Zip),
            // .jar、.war、.apk 本质上是 ZIP 格式
            "jar" | "war" | "apk" => Some(Self::Zip),
            "tar" => Some(Self::Tar),
            "gz" => {
                // 检查是否为 .tar.gz
                let stem = Path::new(&lower).file_stem().and_then(|s| s.to_str())?;
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
        matches!(self, Self::OfficeDocx | Self::OfficeXlsx | Self::OfficePptx)
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
                    &format!(
                        "Cannot extract '{}': unsupported archive type",
                        archive_path
                    ),
                    Some(archive_path.to_string()),
                )],
            }
        }
    };

    match archive_type {
        ArchiveType::Zip
        | ArchiveType::OfficeDocx
        | ArchiveType::OfficeXlsx
        | ArchiveType::OfficePptx => extract_zip(archive_path, policy, &archive_type),
        ArchiveType::Tar => extract_tar(archive_path, policy, false),
        ArchiveType::TarGz => extract_tar(archive_path, policy, true),
        ArchiveType::TarBz2 | ArchiveType::TarXz => {
            // TAR.bz2 和 TAR.xz 暂不支持
            ExtractionResult {
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
            }
        }
    }
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

        // ── 路径安全检查（穿越 + 绝对路径） ──
        if !is_path_safe(&entry_name, temp_dir.path()) {
            findings.push(make_finding(
                "ARCHIVE_PATH_TRAVERSAL",
                IssueSeverity::Critical,
                ThreatCategory::SensitiveFileAccess,
                "Unsafe path detected in archive entry",
                &format!(
                    "Archive entry '{}' contains unsafe path components (.., absolute path, or symlink target)",
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
                        entry_name,
                        ratio,
                        policy.archive.max_compression_ratio,
                        compressed,
                        uncompressed
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

        if is_executable_archive_entry(&entry_name) {
            findings.push(make_finding(
                "ARCHIVE_CONTAINS_EXECUTABLE",
                IssueSeverity::High,
                ThreatCategory::RemoteExec,
                "Archive contains executable file",
                &format!("Archive entry '{}' appears to be an executable", entry_name),
                Some(entry_name.clone()),
            ));
        }

        // ── 记录嵌套归档 ──
        if is_archive_extension(&entry_name) {
            nested_archives.push(entry_name.clone());
        }

        // ── 跳过目录条目 ──
        if entry.is_dir() {
            // 确保目录存在（路径已经过安全检查）
            let out_path = temp_dir.path().join(entry.name());
            if let Err(e) = fs::create_dir_all(&out_path) {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to create directory for archive entry",
                    &format!("Cannot create directory '{}': {}", out_path.display(), e),
                    Some(entry_name.clone()),
                ));
            }
            continue;
        }

        // ── Symlink 检测 ──
        // 多重检测：unix_mode 位 + 文件名启发式
        let is_symlink = if let Some(mode) = entry.unix_mode() {
            // 0xA000 是 symlink 的文件类型位
            mode & 0xA000 == 0xA000
        } else {
            // Windows 上 unix_mode() 返回 None，使用启发式检测
            // 检查是否为常见的 symlink 文件名模式
            entry_name.ends_with(".lnk") || entry_name.contains("->")
        };

        if is_symlink {
            findings.push(make_finding(
                "ARCHIVE_SYMLINK",
                IssueSeverity::Critical,
                ThreatCategory::Destructive,
                "Symlink detected in archive entry",
                &format!(
                    "Entry '{}' is a symbolic link which may point outside the archive",
                    entry_name
                ),
                Some(entry_name.clone()),
            ));
            continue;
        }

        // ── 单条目大小限制 ──
        let entry_size = entry.size();
        let max_single_entry_size = policy.archive.max_total_size_bytes / 4; // 单条目最大为总限制的 1/4
        if entry_size > max_single_entry_size {
            findings.push(make_finding(
                "ARCHIVE_ENTRY_TOO_LARGE",
                IssueSeverity::High,
                ThreatCategory::Obfuscation,
                "Single archive entry exceeds size limit",
                &format!(
                    "Entry '{}' is {} bytes, exceeding single entry limit of {} bytes",
                    entry_name, entry_size, max_single_entry_size
                ),
                Some(entry_name.clone()),
            ));
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
                        &format!("Cannot extract '{}': {}", entry_name, e),
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
                    &format!("Cannot create file '{}': {}", out_path.display(), e),
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
    archive_exts
        .iter()
        .any(|ext| lower.ends_with(&format!(".{}", ext)))
}

/// 计算嵌套归档的最大深度
///
/// 算法：通过路径层级分析来估算嵌套深度
/// 1. 对每个归档条目，计算其路径深度（目录层级数）
/// 2. 如果一个归档路径包含另一个归档路径作为前缀，则增加深度
/// 3. 返回最大检测到的深度
fn calculate_nested_depth(nested_archives: &[String]) -> usize {
    if nested_archives.is_empty() {
        return 0;
    }

    // 提取每个归档条目的目录路径部分
    let archive_paths: Vec<String> = nested_archives
        .iter()
        .filter_map(|path| {
            let p = Path::new(path);
            // 获取目录部分（去掉文件名）
            p.parent().map(|parent| {
                let parent_str = parent.to_string_lossy().to_string();
                if parent_str.is_empty() {
                    ".".to_string()
                } else {
                    parent_str
                }
            })
        })
        .collect();

    // 计算路径深度（目录层级数）
    let max_depth = archive_paths
        .iter()
        .map(|path| path.matches('/').count() + path.matches('\\').count())
        .max()
        .unwrap_or(0);

    // 检查是否有嵌套关系（一个路径是另一个的子路径）
    let mut has_nesting = false;
    for (i, outer) in archive_paths.iter().enumerate() {
        for (j, inner) in archive_paths.iter().enumerate() {
            if i != j && inner.starts_with(outer.as_str()) && inner.len() > outer.len() {
                has_nesting = true;
                break;
            }
        }
        if has_nesting {
            break;
        }
    }

    // 返回深度：基础深度 1 + 嵌套检测加成
    if has_nesting {
        (max_depth + 1).min(3) // 最大限制为 3 层
    } else {
        if max_depth > 0 {
            1 // 有路径但无嵌套
        } else {
            nested_archives.len().min(2) // 根据数量估算
        }
    }
}

// ── TAR 提取逻辑 ──

fn extract_tar(archive_path: &str, policy: &ScanPolicy, is_gzipped: bool) -> ExtractionResult {
    use tar::Archive;

    let mut findings = Vec::new();

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
                extracted_files: Vec::new(),
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
                &format!("Cannot create temp dir: {}", e),
                Some(archive_path.to_string()),
            ));
            return ExtractionResult {
                temp_dir: None,
                extracted_files: Vec::new(),
                findings,
            };
        }
    };

    if is_gzipped {
        use flate2::read::GzDecoder;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        let (files, mut entry_findings) =
            extract_tar_entries(&mut archive, archive_path, policy, &temp_dir);
        findings.append(&mut entry_findings);
        ExtractionResult {
            temp_dir: Some(temp_dir),
            extracted_files: files,
            findings,
        }
    } else {
        let mut archive = Archive::new(file);
        let (files, mut entry_findings) =
            extract_tar_entries(&mut archive, archive_path, policy, &temp_dir);
        findings.append(&mut entry_findings);
        ExtractionResult {
            temp_dir: Some(temp_dir),
            extracted_files: files,
            findings,
        }
    }
}

/// 通用 TAR 条目处理（gzipped 和非 gzipped 共用）
fn extract_tar_entries<R: io::Read>(
    archive: &mut tar::Archive<R>,
    archive_path: &str,
    policy: &ScanPolicy,
    temp_dir: &TempDir,
) -> (Vec<String>, Vec<Finding>) {
    let mut findings = Vec::new();
    let mut extracted_files = Vec::new();
    let mut total_size: u64 = 0;
    let mut file_count: usize = 0;

    let entries = match archive.entries() {
        Ok(e) => e,
        Err(e) => {
            findings.push(make_finding(
                "ARCHIVE_EXTRACTION_FAILED",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Failed to read TAR archive entries",
                &format!("Cannot read entries: {}", e),
                Some(archive_path.to_string()),
            ));
            return (extracted_files, findings);
        }
    };

    for entry in entries {
        let mut entry = match entry {
            Ok(e) => e,
            Err(e) => {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to read TAR entry",
                    &format!("Cannot read entry: {}", e),
                    None,
                ));
                continue;
            }
        };

        let entry_path = match entry.path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(e) => {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to read TAR entry path",
                    &format!("Cannot read entry path: {}", e),
                    None,
                ));
                continue;
            }
        };

        // 路径安全检查（穿越 + 绝对路径）
        if !is_path_safe(&entry_path, temp_dir.path()) {
            findings.push(make_finding(
                "ARCHIVE_PATH_TRAVERSAL",
                IssueSeverity::Critical,
                ThreatCategory::Destructive,
                "Unsafe path in archive entry",
                &format!("Entry '{}' contains unsafe path components", entry_path),
                Some(entry_path.clone()),
            ));
            continue;
        }

        // TAR symlink 检测
        if is_tar_symlink(&entry) {
            findings.push(make_finding(
                "ARCHIVE_SYMLINK",
                IssueSeverity::Critical,
                ThreatCategory::Destructive,
                "Symlink detected in TAR entry",
                &format!(
                    "Entry '{}' is a symbolic link which may point outside the archive",
                    entry_path
                ),
                Some(entry_path.clone()),
            ));
            continue;
        }

        file_count += 1;
        if file_count > policy.archive.max_file_count {
            findings.push(make_finding(
                "ARCHIVE_TOO_MANY_FILES",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Archive contains too many files",
                &format!(
                    "File count exceeds limit ({})",
                    policy.archive.max_file_count
                ),
                Some(archive_path.to_string()),
            ));
            break;
        }

        let out_path = temp_dir.path().join(&entry_path);

        if entry.header().entry_type().is_dir() {
            if let Err(e) = fs::create_dir_all(&out_path) {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to create directory for TAR entry",
                    &format!("Cannot create directory '{}': {}", out_path.display(), e),
                    Some(entry_path.clone()),
                ));
            }
            continue;
        }

        // 创建父目录
        if let Some(parent) = out_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to create parent directory",
                    &format!("Cannot create directory '{}': {}", parent.display(), e),
                    Some(entry_path.clone()),
                ));
                continue;
            }
        }

        // 检查文件大小
        let entry_size = entry.header().size().unwrap_or(0);
        total_size += entry_size;
        if total_size > policy.archive.max_total_size_bytes {
            findings.push(make_finding(
                "ARCHIVE_TOO_LARGE",
                IssueSeverity::Medium,
                ThreatCategory::Obfuscation,
                "Archive uncompressed size exceeds limit",
                &format!(
                    "Total size exceeds limit ({})",
                    policy.archive.max_total_size_bytes
                ),
                Some(archive_path.to_string()),
            ));
            break;
        }

        // 解压文件
        match fs::File::create(&out_path) {
            Ok(mut out_file) => {
                if let Err(e) = io::copy(&mut entry, &mut out_file) {
                    findings.push(make_finding(
                        "ARCHIVE_EXTRACTION_FAILED",
                        IssueSeverity::Medium,
                        ThreatCategory::Obfuscation,
                        "Failed to extract TAR entry",
                        &format!("Cannot extract '{}': {}", entry_path, e),
                        Some(entry_path.clone()),
                    ));
                    continue;
                }
            }
            Err(e) => {
                findings.push(make_finding(
                    "ARCHIVE_EXTRACTION_FAILED",
                    IssueSeverity::Medium,
                    ThreatCategory::Obfuscation,
                    "Failed to create output file",
                    &format!("Cannot create file '{}': {}", out_path.display(), e),
                    Some(entry_path.clone()),
                ));
                continue;
            }
        }

        extracted_files.push(out_path.to_string_lossy().to_string());
    }

    (extracted_files, findings)
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
    // 根据规则类型确定 FindingKind
    let finding_kind = match rule_id {
        // 路径穿越、symlink、zip bomb 是安全风险
        "ARCHIVE_PATH_TRAVERSAL" | "ARCHIVE_SYMLINK" | "ARCHIVE_ZIP_BOMB" => {
            FindingKind::Security
        }
        // 其他归档问题是不可审计性
        _ => FindingKind::Auditability,
    };

    finding_builder::make_finding(FindingSpec {
        rule_id,
        category,
        severity,
        title,
        description: description.to_string(),
        file_path,
        line_number: None,
        snippet: None,
        remediation: Some(remediation_for_rule(rule_id).to_string()),
        analyzer: "archive_extractor",
        finding_kind,
        rule_source: None,
        cwe_id: None,
        confidence: None,
        id_salt: None,
    })
}

/// 为每条规则提供修复建议
fn is_executable_archive_entry(name: &str) -> bool {
    let lower = name.to_lowercase();
    [
        ".exe", ".dll", ".bat", ".cmd", ".ps1", ".msi", ".com", ".scr", ".sh", ".bin",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

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
        "ARCHIVE_SYMLINK" => {
            "Archive contains symbolic links which may point outside the archive. \
             Remove symlinks or verify they point to safe targets within the archive."
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
        let tmp = tempfile::Builder::new().suffix(".zip").tempfile().unwrap();
        let file = fs::File::create(tmp.path()).unwrap();
        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

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
        assert_eq!(
            detect_archive_type("file.docx"),
            Some(ArchiveType::OfficeDocx)
        );
        assert_eq!(
            detect_archive_type("file.xlsx"),
            Some(ArchiveType::OfficeXlsx)
        );
        assert_eq!(
            detect_archive_type("file.pptx"),
            Some(ArchiveType::OfficePptx)
        );
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
            result
                .findings
                .iter()
                .all(|f| f.severity != IssueSeverity::Critical),
            "No critical findings expected for normal ZIP"
        );
    }

    #[test]
    fn test_extract_zip_path_traversal() {
        // 路径穿越：条目名包含 ..
        let entries = vec![
            (
                "../../etc/passwd",
                b"root:x:0:0:root:/root:/bin/bash" as &[u8],
            ),
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
        let has_passwd = result.extracted_files.iter().any(|f| f.contains("passwd"));
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
        let tmp = tempfile::Builder::new().suffix(".zip").tempfile().unwrap();
        let file = fs::File::create(tmp.path()).unwrap();
        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

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
            result
                .findings
                .iter()
                .any(|f| f.rule_id == "ARCHIVE_UNSUPPORTED"),
            "Should report unsupported format"
        );
    }

    #[test]
    fn test_extract_nonexistent_file() {
        let result = extract_archive("/tmp/nonexistent_12345.zip", &default_policy());
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.rule_id == "ARCHIVE_EXTRACTION_FAILED"),
            "Should report extraction failure for missing file"
        );
    }

    #[test]
    fn test_extract_corrupted_zip() {
        let tmp = tempfile::Builder::new().suffix(".zip").tempfile().unwrap();
        fs::write(tmp.path(), b"this is not a zip file").unwrap();

        let result = extract_archive(tmp.path().to_str().unwrap(), &default_policy());
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.rule_id == "ARCHIVE_EXTRACTION_FAILED"),
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
