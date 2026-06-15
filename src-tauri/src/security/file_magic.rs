//! 文件 Magic 检测模块（File Magic Checker）
//!
//! 检测文件扩展名与实际内容类型的一致性，防止二进制/脚本文件
//! 伪装成文本文件（如 .py/.md）绕过安全检查。
//!
//! 纯 Rust 实现，不依赖外部 libmagic 库。

use std::path::Path;

use crate::models::security::{Finding, FindingKind, IssueSeverity, ThreatCategory};
use crate::security::finding_builder::{self, FindingSpec};

const ANALYZER_NAME: &str = "file_magic";

// ── 检测到的内容类型 ──

/// 检测到的内容类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    /// Windows PE (.exe, .dll)
    Pe,
    /// Linux ELF
    Elf,
    /// macOS Mach-O
    MachO,
    /// ZIP 归档
    Zip,
    /// PDF 文档
    Pdf,
    /// Office (OLE2)
    Office,
    /// Office Open XML (.docx, .xlsx)
    OfficeXml,
    /// gzip 压缩
    Gzip,
    /// tar 归档
    Tar,
    /// Shell 脚本 (shebang)
    ShellScript,
    /// Python 脚本 (shebang)
    PythonScript,
    /// JavaScript
    JavaScript,
    /// HTML
    Html,
    /// SVG (含脚本)
    Svg,
    /// PNG 图片
    Png,
    /// JPEG 图片
    Jpeg,
    /// GIF 图片
    Gif,
    /// 纯文本
    Text,
    /// 未知类型
    Unknown,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Pe => "PE executable",
            ContentType::Elf => "ELF executable",
            ContentType::MachO => "Mach-O executable",
            ContentType::Zip => "ZIP archive",
            ContentType::Pdf => "PDF document",
            ContentType::Office => "Office document (OLE2)",
            ContentType::OfficeXml => "Office document (OOXML)",
            ContentType::Gzip => "gzip archive",
            ContentType::Tar => "tar archive",
            ContentType::ShellScript => "shell script",
            ContentType::PythonScript => "Python script",
            ContentType::JavaScript => "JavaScript",
            ContentType::Html => "HTML",
            ContentType::Svg => "SVG",
            ContentType::Png => "PNG image",
            ContentType::Jpeg => "JPEG image",
            ContentType::Gif => "GIF image",
            ContentType::Text => "text file",
            ContentType::Unknown => "unknown",
        }
    }
}

// ── Magic byte 检测 ──

/// 检测文件内容类型（基于 magic bytes，仅检查前 512 字节）
pub fn detect_content_type(data: &[u8]) -> ContentType {
    if data.is_empty() {
        return ContentType::Unknown;
    }

    // 检查各 magic bytes 签名

    // PE: "MZ"
    if data.len() >= 2 && data[0] == 0x4D && data[1] == 0x5A {
        return ContentType::Pe;
    }

    // ELF: \x7fELF
    if data.len() >= 4 && data[0] == 0x7F && data[1] == b'E' && data[2] == b'L' && data[3] == b'F' {
        return ContentType::Elf;
    }

    // Mach-O: \xfe\xed\xfa (big-endian) 或 \xcf\xfa\xed\xfe (little-endian)
    if data.len() >= 4 {
        if (data[0] == 0xFE && data[1] == 0xED && data[2] == 0xFA)
            || (data[0] == 0xCF && data[1] == 0xFA && data[2] == 0xED && data[3] == 0xFE)
        {
            return ContentType::MachO;
        }
    }

    // ZIP: PK\x03\x04
    if data.len() >= 4 && data[0] == b'P' && data[1] == b'K' && data[2] == 0x03 && data[3] == 0x04 {
        return ContentType::Zip;
    }

    // PDF: %PDF
    if data.len() >= 4 && data[0] == b'%' && data[1] == b'P' && data[2] == b'D' && data[3] == b'F' {
        return ContentType::Pdf;
    }

    // Office OLE2: \xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1
    if data.len() >= 8
        && data[0] == 0xD0
        && data[1] == 0xCF
        && data[2] == 0x11
        && data[3] == 0xE0
        && data[4] == 0xA1
        && data[5] == 0xB1
        && data[6] == 0x1A
        && data[7] == 0xE1
    {
        return ContentType::Office;
    }

    // gzip: \x1f\x8b
    if data.len() >= 2 && data[0] == 0x1F && data[1] == 0x8B {
        return ContentType::Gzip;
    }

    // PNG: \x89PNG\r\n\x1a\n
    if data.len() >= 8 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        return ContentType::Png;
    }

    // JPEG: \xff\xd8\xff
    if data.len() >= 3 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return ContentType::Jpeg;
    }

    // GIF: GIF87a / GIF89a
    if data.len() >= 6 && (&data[..6] == b"GIF87a" || &data[..6] == b"GIF89a") {
        return ContentType::Gif;
    }

    // tar: "ustar" at offset 257
    if data.len() >= 263 && &data[257..262] == b"ustar" {
        return ContentType::Tar;
    }

    // 文本类检测（需要先确认是文本内容）
    if looks_like_text(data) {
        // SVG: 检查 <svg 标签（可能在 XML/HTML 中）
        if has_svg_tag(data) {
            return ContentType::Svg;
        }

        // HTML: 检查 <!DOCTYPE html 或 <html
        if has_html_signature(data) {
            return ContentType::Html;
        }

        // shebang: 检查 #! 开头
        if data.len() >= 2 && data[0] == b'#' && data[1] == b'!' {
            return detect_shebang(data);
        }

        return ContentType::Text;
    }

    // Office Open XML: 本质是 ZIP，通过扩展名区分
    // 这里已经返回了 Zip，由 check_magic 根据扩展名进一步判断

    ContentType::Unknown
}

/// 检查数据是否看起来像文本（前 512 字节中无大量二进制字节）
fn looks_like_text(data: &[u8]) -> bool {
    let sample_len = data.len().min(512);
    let mut non_ascii = 0usize;
    for &b in &data[..sample_len] {
        // 允许常见控制字符：\n \r \t
        if b == b'\n' || b == b'\r' || b == b'\t' {
            continue;
        }
        if b == 0 {
            return false; // NUL 字节说明是二进制
        }
        if !b.is_ascii() {
            non_ascii += 1;
        }
    }
    // 非 ASCII 字节占比不超过 30%
    (non_ascii as f64 / sample_len as f64) < 0.30
}

/// 检查是否包含 SVG 标签
fn has_svg_tag(data: &[u8]) -> bool {
    let sample_len = data.len().min(512);
    let text = String::from_utf8_lossy(&data[..sample_len]);
    let lower = text.to_ascii_lowercase();

    // 查找 <svg 标签（带或不带空格/属性）
    if let Some(pos) = lower.find("<svg") {
        let after = &lower[pos + 4..];
        // <svg 后面要么是空白字符、>、/ 或结束
        after.is_empty()
            || after.starts_with(' ')
            || after.starts_with('\t')
            || after.starts_with('\n')
            || after.starts_with('\r')
            || after.starts_with('>')
            || after.starts_with('/')
    } else {
        false
    }
}

/// 检查是否包含 HTML 签名
fn has_html_signature(data: &[u8]) -> bool {
    let sample_len = data.len().min(512);
    let text = String::from_utf8_lossy(&data[..sample_len]);
    let lower = text.to_ascii_lowercase();
    let trimmed = lower.trim_start();

    trimmed.starts_with("<!doctype html") || trimmed.starts_with("<html")
}

/// 检测 shebang 行的具体脚本类型
fn detect_shebang(data: &[u8]) -> ContentType {
    // 找到第一行（shebang 行）
    let first_line_end = data.iter().position(|&b| b == b'\n').unwrap_or(data.len());
    let shebang_line = &data[..first_line_end];

    let line = String::from_utf8_lossy(shebang_line);
    let line = line.trim();

    // #!/usr/bin/env python3 / #!/usr/bin/python3 / #!/usr/bin/env python
    if line.contains("python") {
        return ContentType::PythonScript;
    }

    // #!/usr/bin/env node / #!/usr/bin/node
    if line.contains("node") {
        return ContentType::JavaScript;
    }

    // 默认归类为 shell 脚本
    // #!/bin/bash, #!/bin/sh, #!/usr/bin/env bash 等
    ContentType::ShellScript
}

// ── 一致性检查 ──

/// 检查扩展名与内容类型是否一致，不一致则返回 Finding
pub fn check_magic(file_path: &str, data: &[u8]) -> Option<Finding> {
    let ext = Path::new(file_path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())?;

    let detected = detect_content_type(data);
    let expected_severity = mismatch_severity(&ext, detected)?;

    let expected_desc = describe_expected_for_ext(&ext);
    let actual_desc = detected.as_str();

    let finding = make_finding(file_path, &expected_desc, actual_desc, expected_severity);

    Some(finding)
}

/// 根据扩展名和检测到的类型判断是否不匹配，返回严重度
fn mismatch_severity(ext: &str, detected: ContentType) -> Option<IssueSeverity> {
    use ContentType::*;

    match ext {
        // 脚本/文本扩展名：如果是二进制/归档类内容 → Critical
        "py" | "pyw" | "pyi" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "md" | "json"
        | "yaml" | "yml" | "toml" | "cfg" | "ini" | "conf" | "xml" | "txt" | "csv" => {
            match detected {
                Pe | Elf | MachO | Zip | Pdf | Office | OfficeXml | Gzip | Tar | Png | Jpeg
                | Gif => {
                    Some(IssueSeverity::Critical)
                }
                Html | Svg => Some(IssueSeverity::High),
                _ => None, // Text, ShellScript, PythonScript, JavaScript, Unknown → OK
            }
        }

        // 图片扩展名：如果内容是脚本/可执行 → High
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" | "avif" => match detected
        {
            Pe | Elf | MachO | Zip | Pdf | Office | OfficeXml | Gzip | Tar | ShellScript
            | PythonScript | JavaScript | Html | Svg => Some(IssueSeverity::High),
            _ => None,
        },

        // 可执行扩展名：这些扩展名本就可能是可执行 → 不检查
        "exe" | "dll" | "so" | "dylib" | "bin" => None,

        // 归档扩展名：本就可能是归档 → 不检查
        "zip" | "tar" | "gz" | "tgz" => None,

        // 其他扩展名：不做检查
        _ => None,
    }
}

/// 根据扩展名描述期望的内容类型
fn describe_expected_for_ext(ext: &str) -> String {
    match ext {
        "py" | "pyw" | "pyi" => "Python source".to_string(),
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript source".to_string(),
        "ts" | "tsx" => "TypeScript source".to_string(),
        "md" => "Markdown document".to_string(),
        "json" => "JSON data".to_string(),
        "yaml" | "yml" => "YAML data".to_string(),
        "toml" => "TOML data".to_string(),
        "cfg" | "ini" | "conf" => "Configuration file".to_string(),
        "xml" => "XML document".to_string(),
        "txt" | "csv" => "Text file".to_string(),
        "png" => "PNG image".to_string(),
        "jpg" | "jpeg" => "JPEG image".to_string(),
        "gif" => "GIF image".to_string(),
        "webp" => "WebP image".to_string(),
        "bmp" => "Bitmap image".to_string(),
        "ico" => "Icon file".to_string(),
        "tiff" => "TIFF image".to_string(),
        "avif" => "AVIF image".to_string(),
        _ => format!("{} file", ext),
    }
}

/// 构造 Finding
fn make_finding(file_path: &str, expected: &str, actual: &str, severity: IssueSeverity) -> Finding {
    finding_builder::make_finding(FindingSpec {
        rule_id: "FILE_MAGIC_MISMATCH",
        category: ThreatCategory::Obfuscation,
        severity,
        title: "File extension/content type mismatch",
        description: format!(
            "File '{}' appears to be {} but has extension suggesting {}",
            file_path, actual, expected
        ),
        file_path: Some(file_path.to_string()),
        line_number: None,
        snippet: None,
        remediation: Some(format!(
            "Rename the file to match its actual content type or remove it if it is malicious."
        )),
        analyzer: ANALYZER_NAME,
        finding_kind: FindingKind::Security,
        rule_source: None,
        cwe_id: Some("CWE-434".to_string()),
        confidence: Some("High".to_string()),
        id_salt: None,
    })
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pe() {
        let data = b"MZ\x90\x00\x03\x00some pe content here";
        assert_eq!(detect_content_type(data), ContentType::Pe);
    }

    #[test]
    fn test_detect_elf() {
        let data = b"\x7fELF\x02\x01\x01some elf content";
        assert_eq!(detect_content_type(data), ContentType::Elf);
    }

    #[test]
    fn test_detect_macho_big_endian() {
        let data = b"\xfe\xed\xfa\xce\x00some macho content";
        assert_eq!(detect_content_type(data), ContentType::MachO);
    }

    #[test]
    fn test_detect_macho_little_endian() {
        let data = b"\xcf\xfa\xed\xfe\x00some macho content";
        assert_eq!(detect_content_type(data), ContentType::MachO);
    }

    #[test]
    fn test_detect_zip() {
        let data = b"PK\x03\x04some zip content here";
        assert_eq!(detect_content_type(data), ContentType::Zip);
    }

    #[test]
    fn test_detect_pdf() {
        let data = b"%PDF-1.4 some pdf content";
        assert_eq!(detect_content_type(data), ContentType::Pdf);
    }

    #[test]
    fn test_detect_office_ole2() {
        let data = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1some office content";
        assert_eq!(detect_content_type(data), ContentType::Office);
    }

    #[test]
    fn test_detect_gzip() {
        let data = b"\x1f\x8b\x08some gzip content here";
        assert_eq!(detect_content_type(data), ContentType::Gzip);
    }

    #[test]
    fn test_detect_tar() {
        let mut data = vec![0u8; 263];
        data[257..262].copy_from_slice(b"ustar");
        assert_eq!(detect_content_type(&data), ContentType::Tar);
    }

    #[test]
    fn test_detect_shell_shebang() {
        let data = b"#!/bin/bash\necho hello";
        assert_eq!(detect_content_type(data), ContentType::ShellScript);
    }

    #[test]
    fn test_detect_python_shebang() {
        let data = b"#!/usr/bin/env python3\nprint('hello')";
        assert_eq!(detect_content_type(data), ContentType::PythonScript);
    }

    #[test]
    fn test_detect_node_shebang() {
        let data = b"#!/usr/bin/env node\nconsole.log('hello')";
        assert_eq!(detect_content_type(data), ContentType::JavaScript);
    }

    #[test]
    fn test_detect_html() {
        let data = b"<!DOCTYPE html>\n<html><body>Hello</body></html>";
        assert_eq!(detect_content_type(data), ContentType::Html);
    }

    #[test]
    fn test_detect_html_lowercase() {
        let data = b"<html><body>Hello</body></html>";
        assert_eq!(detect_content_type(data), ContentType::Html);
    }

    #[test]
    fn test_detect_svg() {
        let data = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"10\"/></svg>";
        assert_eq!(detect_content_type(data), ContentType::Svg);
    }

    #[test]
    fn test_detect_text() {
        let data = b"Hello, this is a plain text file.\nWith multiple lines.";
        assert_eq!(detect_content_type(data), ContentType::Text);
    }

    #[test]
    fn test_detect_empty() {
        assert_eq!(detect_content_type(b""), ContentType::Unknown);
    }

    // ── 一致性检查测试 ──

    #[test]
    fn test_pe_disguised_as_py_critical() {
        let data = b"MZ\x90\x00\x03\x00some pe content";
        let finding = check_magic("malware.py", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
        assert_eq!(finding.rule_id, "FILE_MAGIC_MISMATCH");
        assert!(finding.description.contains("PE executable"));
        assert!(finding.description.contains("Python source"));
    }

    #[test]
    fn test_zip_disguised_as_md_critical() {
        let data = b"PK\x03\x04some zip content";
        let finding = check_magic("secret.md", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
        assert!(finding.description.contains("ZIP archive"));
        assert!(finding.description.contains("Markdown document"));
    }

    #[test]
    fn test_png_disguised_as_md_critical() {
        let data = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        let finding = check_magic("notes.md", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
        assert!(finding.description.contains("PNG image"));
        assert!(finding.description.contains("Markdown document"));
    }

    #[test]
    fn test_html_disguised_as_png_high() {
        let data = b"<!DOCTYPE html>\n<html><body>phish</body></html>";
        let finding = check_magic("image.png", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::High);
        assert!(finding.description.contains("HTML"));
        assert!(finding.description.contains("PNG image"));
    }

    #[test]
    fn test_pe_disguised_as_js_critical() {
        let data = b"MZ\x90\x00\x03\x00some pe content";
        let finding = check_magic("payload.js", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
    }

    #[test]
    fn test_svg_disguised_as_py_high() {
        let data = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>";
        let finding = check_magic("legit.py", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::High);
        assert!(finding.description.contains("SVG"));
    }

    #[test]
    fn test_normal_py_no_finding() {
        let data = b"#!/usr/bin/env python3\nprint('hello')\n";
        assert!(check_magic("script.py", data).is_none());
    }

    #[test]
    fn test_normal_png_no_finding() {
        // PNG magic bytes
        let data = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
        assert!(check_magic("image.png", data).is_none());
    }

    #[test]
    fn test_normal_md_no_finding() {
        let data = b"# Hello World\n\nThis is markdown.\n";
        assert!(check_magic("readme.md", data).is_none());
    }

    #[test]
    fn test_elf_disguised_as_md_critical() {
        let data = b"\x7fELF\x02\x01\x01some elf content";
        let finding = check_magic("notes.md", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
    }

    #[test]
    fn test_macho_disguised_as_txt_critical() {
        let data = b"\xfe\xed\xfa\xce\x00some macho content";
        let finding = check_magic("data.txt", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
    }

    #[test]
    fn test_gzip_disguised_as_json_critical() {
        let data = b"\x1f\x8b\x08some gzip content here";
        let finding = check_magic("config.json", data).expect("should detect mismatch");
        assert_eq!(finding.severity, IssueSeverity::Critical);
    }

    #[test]
    fn test_no_finding_for_exe_extension() {
        // PE in .exe → expected, no mismatch
        let data = b"MZ\x90\x00\x03\x00some pe content";
        assert!(check_magic("program.exe", data).is_none());
    }

    #[test]
    fn test_no_finding_for_zip_extension() {
        let data = b"PK\x03\x04some zip content";
        assert!(check_magic("archive.zip", data).is_none());
    }

    #[test]
    fn test_unknown_content_no_finding() {
        // Random bytes that don't match any known type
        let data = b"\x01\x02\x03\x04\x05\x06\x07\x08";
        assert!(check_magic("data.py", data).is_none());
    }

    #[test]
    fn test_finding_metadata_has_cwe() {
        let data = b"MZ\x90\x00\x03\x00some pe content";
        let finding = check_magic("test.py", data).expect("should detect mismatch");
        let meta = finding.metadata.expect("should have metadata");
        assert_eq!(meta.cwe_id.as_deref(), Some("CWE-434"));
        assert_eq!(meta.confidence.as_deref(), Some("High"));
    }

    #[test]
    fn test_finding_has_remediation() {
        let data = b"MZ\x90\x00\x03\x00some pe content";
        let finding = check_magic("test.py", data).expect("should detect mismatch");
        assert!(finding.remediation.is_some());
    }
}
