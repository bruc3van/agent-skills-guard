//! 结构校验模块（Strict Structure Validation）
//!
//! 对 Skill 目录进行白名单校验：SKILL.md 存在性、frontmatter 格式、
//! name/description 约束、隐藏文件、不允许的子目录/扩展名、二进制内容等。
//!
//! `SingleFile` 模式跳过所有结构检查，直接返回空 `Vec`。

use sha2::{Digest, Sha256};

use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use crate::security::skill_context::{ScanMode, SkillContext, SkillFileType};

// ── 常量 ──

const ANALYZER_NAME: &str = "strict_structure";

const NAME_MIN_LEN: usize = 2;
const NAME_MAX_LEN: usize = 64;
const DESC_MIN_LEN: usize = 10;
const DESC_MAX_LEN: usize = 1024;

// ── 公共接口 ──

/// 对 SkillContext 执行结构校验，返回所有 Finding
pub fn validate(ctx: &SkillContext) -> Vec<Finding> {
    // SingleFile 模式跳过所有结构检查
    if ctx.scan_mode == ScanMode::SingleFile {
        return Vec::new();
    }

    let mut findings = Vec::new();

    // 1. 检查 SKILL.md 存在性
    if ctx.skill_md_path.is_none() {
        findings.push(make_finding(
            "STRUCTURE_MISSING_SKILL_MD",
            IssueSeverity::High,
            "Skill directory is missing SKILL.md".to_string(),
            None,
            None,
        ));
        // 缺少 SKILL.md 时后续检查无意义，直接返回
        return findings;
    }

    // 2. 检查 frontmatter 解析
    if ctx.manifest.is_none() {
        findings.push(make_finding(
            "FRONTMATTER_PARSE_ERROR",
            IssueSeverity::Medium,
            "Failed to parse YAML front-matter in SKILL.md".to_string(),
            ctx.skill_md_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            None,
        ));
    }

    // 3. 校验 name 格式
    if let Some(ref manifest) = ctx.manifest {
        if !manifest.name.is_empty() && !is_valid_name(&manifest.name) {
            findings.push(make_finding(
                "STRUCTURE_INVALID_NAME",
                IssueSeverity::Medium,
                format!(
                    "Invalid skill name '{}': must be lowercase alphanumeric, 2-64 chars, \
                     single hyphens only, no leading/trailing hyphens",
                    manifest.name
                ),
                ctx.skill_md_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        }

        // 3.1 校验 name 与目录名一致性
        if !manifest.name.is_empty() {
            if let Some(ref dir_path) = ctx.skill_dir {
                if let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str()) {
                    if manifest.name != dir_name {
                        findings.push(make_finding(
                            "STRUCTURE_NAME_DIR_MISMATCH",
                            IssueSeverity::Medium,
                            format!(
                                "Skill name '{}' does not match directory name '{}'",
                                manifest.name, dir_name
                            ),
                            ctx.skill_md_path
                                .as_ref()
                                .map(|p| p.to_string_lossy().to_string()),
                            None,
                        ));
                    }
                }
            }
        }

        // 4. 校验 description
        let desc_len = manifest.description.len();
        if desc_len == 0 {
            findings.push(make_finding(
                "STRUCTURE_INVALID_DESCRIPTION",
                IssueSeverity::Medium,
                "Description is empty (must be 10-1024 chars)".to_string(),
                ctx.skill_md_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        } else if desc_len < DESC_MIN_LEN {
            findings.push(make_finding(
                "STRUCTURE_INVALID_DESCRIPTION",
                IssueSeverity::Medium,
                format!(
                    "Description is too short ({} chars, minimum {})",
                    desc_len, DESC_MIN_LEN
                ),
                ctx.skill_md_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        } else if desc_len > DESC_MAX_LEN {
            findings.push(make_finding(
                "STRUCTURE_INVALID_DESCRIPTION",
                IssueSeverity::Medium,
                format!(
                    "Description is too long ({} chars, maximum {})",
                    desc_len, DESC_MAX_LEN
                ),
                ctx.skill_md_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        }

        // 4.0 许可证字段（可选但建议提供）
        if manifest
            .license
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            findings.push(make_finding(
                "MANIFEST_MISSING_LICENSE",
                IssueSeverity::Low,
                "SKILL.md frontmatter has no license field".to_string(),
                ctx.skill_md_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        }

        // 4.1 校验 compatibility 字段长度
        let compatibility_total_len: usize = manifest.compatibility.values().map(|v| v.len()).sum();
        if compatibility_total_len > 500 {
            findings.push(make_finding(
                "STRUCTURE_COMPATIBILITY_TOO_LONG",
                IssueSeverity::Low,
                format!(
                    "Compatibility field is too long ({} chars, maximum 500)",
                    compatibility_total_len
                ),
                ctx.skill_md_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                None,
            ));
        }
    }

    // 遍历文件列表做文件级校验
    let policy = &ctx.scan_policy.strict_structure;
    for file in &ctx.files {
        let rel = file.relative_path.to_string_lossy().to_string();

        // 5. 隐藏文件检查
        if file.is_hidden {
            findings.push(make_finding(
                "STRUCTURE_HIDDEN_FILE",
                IssueSeverity::Medium,
                format!("Hidden file detected: {}", rel),
                Some(rel.clone()),
                None,
            ));
            let ext = file
                .absolute_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());
            let is_script = matches!(
                ext.as_deref(),
                Some("py") | Some("sh") | Some("bash") | Some("js") | Some("ts")
            );
            if is_script {
                findings.push(make_finding(
                    "HIDDEN_EXECUTABLE_SCRIPT",
                    IssueSeverity::High,
                    format!("Hidden executable script: {}", rel),
                    Some(rel.clone()),
                    None,
                ));
            } else if matches!(
                ext.as_deref(),
                Some("db") | Some("sqlite") | Some("sqlite3") | Some("env") | Some("json")
            ) {
                findings.push(make_finding(
                    "HIDDEN_DATA_FILE",
                    IssueSeverity::Medium,
                    format!("Hidden data file: {}", rel),
                    Some(rel.clone()),
                    None,
                ));
            }
        }

        // 5.1 __pycache__ 目录
        if rel.replace('\\', "/").contains("__pycache__/") {
            findings.push(make_finding(
                "PYCACHE_FILES_DETECTED",
                IssueSeverity::Low,
                format!("Python cache directory/file detected: {}", rel),
                Some(rel.clone()),
                None,
            ));
        }

        // 6. 不允许的子目录检查
        //    检查路径中每一级目录是否在白名单中
        if let Some(components) = file.relative_path.parent() {
            for component in components.components() {
                if let Some(dir_name) = component.as_os_str().to_str() {
                    // 跳过当前目录标记
                    if dir_name == "." {
                        continue;
                    }
                    if !policy.allowed_subdirs.contains(dir_name) {
                        findings.push(make_finding(
                            "STRUCTURE_DISALLOWED_SUBDIR",
                            IssueSeverity::Medium,
                            format!("Disallowed subdirectory '{}' in path: {}", dir_name, rel),
                            Some(rel.clone()),
                            None,
                        ));
                        // 只报告一次，避免重复
                        break;
                    }
                }
            }
        }

        // 7. 不允许的扩展名检查
        if let Some(ext) = file.absolute_path.extension().and_then(|e| e.to_str()) {
            let dot_ext = format!(".{}", ext.to_lowercase());
            if !policy.allowed_extensions.contains(dot_ext.as_str()) {
                findings.push(make_finding(
                    "STRUCTURE_DISALLOWED_EXTENSION",
                    IssueSeverity::Medium,
                    format!("Disallowed file extension '{}' in: {}", dot_ext, rel),
                    Some(rel.clone()),
                    None,
                ));
            }
        }

        // 8. 二进制内容检查
        if file.is_binary {
            findings.push(make_finding(
                "STRUCTURE_BINARY_CONTENT",
                IssueSeverity::Low,
                format!("Binary content detected in file: {}", rel),
                Some(rel.clone()),
                None,
            ));
        }

        // 8.1 非 UTF-8 编码检查（仅文本文件）
        if !file.is_binary && file.file_type != SkillFileType::Asset {
            if let Ok(content) = std::fs::read_to_string(&file.absolute_path) {
                // 尝试读取为 UTF-8，如果失败则说明非 UTF-8 编码
                // read_to_string 本身会检查 UTF-8，所以这里不需要额外检查
                // 但如果文件包含无效 UTF-8，read_to_string 会返回 Err
                let _ = content; // 如果成功读取，说明是有效的 UTF-8
            } else {
                findings.push(make_finding(
                    "STRUCTURE_NON_UTF8",
                    IssueSeverity::Low,
                    format!("Text file '{}' is not valid UTF-8", rel),
                    Some(rel.clone()),
                    None,
                ));
            }
        }
    }

    // 9. 孤立脚本检查：脚本文件在 script_files 中但不在 referenced_files 中
    for script_path in &ctx.script_files {
        let script_str = normalize_path(&script_path.to_string_lossy());
        let is_referenced = ctx.referenced_files.iter().any(|ref_path| {
            let ref_str = normalize_path(&ref_path.to_string_lossy());
            ref_str == script_str
                || script_str.ends_with(&ref_str)
                || ref_str.ends_with(&script_str)
        });
        if !is_referenced {
            findings.push(make_finding(
                "STRUCTURE_ORPHAN_SCRIPT",
                IssueSeverity::Low,
                format!(
                    "Script file '{}' is not referenced in SKILL.md",
                    script_path.to_string_lossy()
                ),
                Some(script_path.to_string_lossy().to_string()),
                None,
            ));
        }
    }

    // 10. 缺失引用检查：referenced_files 中的路径在 files 中不存在
    for ref_path in &ctx.referenced_files {
        let ref_str = normalize_path(&ref_path.to_string_lossy());
        let exists = ctx.files.iter().any(|file| {
            let file_str = normalize_path(&file.relative_path.to_string_lossy());
            file_str == ref_str || file_str.ends_with(&ref_str) || ref_str.ends_with(&file_str)
        });
        if !exists {
            findings.push(make_finding(
                "STRUCTURE_MISSING_REFERENCE",
                IssueSeverity::Medium,
                format!(
                    "Referenced file '{}' does not exist in skill directory",
                    ref_path.to_string_lossy()
                ),
                Some(ref_path.to_string_lossy().to_string()),
                None,
            ));
        }
    }

    findings
}

// ── 辅助函数 ──

/// 检查 name 格式是否有效
///
/// 规则：
/// - 仅包含小写字母、数字、连字符
/// - 长度 2-64
/// - 不以连字符开头或结尾
/// - 无连续连字符
pub fn is_valid_name(name: &str) -> bool {
    let len = name.len();
    if len < NAME_MIN_LEN || len > NAME_MAX_LEN {
        return false;
    }

    let bytes = name.as_bytes();

    // 第一个和最后一个字符必须是小写字母或数字
    if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
        return false;
    }
    if !bytes[len - 1].is_ascii_lowercase() && !bytes[len - 1].is_ascii_digit() {
        return false;
    }

    // 检查所有字符以及连续连字符
    let mut prev_is_hyphen = false;
    for &b in bytes {
        match b {
            b'a'..=b'z' | b'0'..=b'9' => prev_is_hyphen = false,
            b'-' => {
                if prev_is_hyphen {
                    return false; // 连续连字符
                }
                prev_is_hyphen = true;
            }
            _ => return false, // 不允许的字符
        }
    }

    true
}

/// 已知二进制扩展名
pub fn is_known_binary_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "dat"
            | "db"
            | "sqlite"
            | "sqlite3"
            | "wasm"
            | "jar"
            | "war"
            | "ear"
            | "pyc"
            | "pyo"
            | "class"
            | "o"
            | "msi"
            | "dmg"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "bmp"
            | "ico"
            | "icns"
            | "tif"
            | "tiff"
            | "heic"
            | "heif"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "eot"
            | "mp3"
            | "mp4"
            | "wav"
            | "ogg"
            | "webm"
            | "flac"
            | "aac"
            | "pdf"
            | "zip"
            | "gz"
            | "tar"
            | "rar"
            | "7z"
            | "bz2"
            | "xz"
    )
}

/// 规范化路径分隔符（将反斜杠转换为正斜杠）
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// 创建一个 Finding 实例
///
/// 使用 sha2 生成稳定的 finding ID：SHA256(rule_id + file + line)[:16]
pub fn make_finding(
    rule_id: &str,
    severity: IssueSeverity,
    description: String,
    file_path: Option<String>,
    line_number: Option<usize>,
) -> Finding {
    // 生成稳定 ID
    let id_input = format!(
        "{}|{}|{}",
        rule_id,
        file_path.as_deref().unwrap_or(""),
        line_number.map(|l| l.to_string()).unwrap_or_default()
    );
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let id = hash[..16].to_string();

    let title = match rule_id {
        "STRUCTURE_MISSING_SKILL_MD" => "Missing SKILL.md",
        "FRONTMATTER_PARSE_ERROR" => "Front-matter parse error",
        "STRUCTURE_INVALID_NAME" => "Invalid skill name",
        "STRUCTURE_INVALID_DESCRIPTION" => "Invalid skill description",
        "STRUCTURE_HIDDEN_FILE" => "Hidden file detected",
        "STRUCTURE_DISALLOWED_SUBDIR" => "Disallowed subdirectory",
        "STRUCTURE_DISALLOWED_EXTENSION" => "Disallowed file extension",
        "STRUCTURE_BINARY_CONTENT" => "Binary content detected",
        "STRUCTURE_ORPHAN_SCRIPT" => "Orphan script file",
        "STRUCTURE_MISSING_REFERENCE" => "Missing referenced file",
        "STRUCTURE_NAME_DIR_MISMATCH" => "Name-directory mismatch",
        "STRUCTURE_NON_UTF8" => "Non-UTF-8 encoding",
        "STRUCTURE_COMPATIBILITY_TOO_LONG" => "Compatibility field too long",
        _ => "Structure violation",
    };

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category: ThreatCategory::PolicyViolation,
        severity,
        title: title.to_string(),
        description,
        file_path,
        line_number,
        snippet: None,
        remediation: Some("Review and correct the skill structure per policy".to_string()),
        analyzer: ANALYZER_NAME.to_string(),
        metadata: Some(FindingMetadata {
            rule_source: Some("strict_structure".to_string()),
            finding_kind: Some(crate::models::security::FindingKind::Structure),
            ..Default::default()
        }),
    }
}

// ── 单元测试 ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::policy::ScanPolicy;
    use crate::security::skill_context::{ScanMode, SkillContext, SkillFile, SkillFileType};
    use std::path::PathBuf;

    fn make_test_ctx(mode: ScanMode, files: Vec<SkillFile>) -> SkillContext {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(mode, Some(PathBuf::from("/tmp/test-skill")), policy);
        ctx.files = files;
        ctx
    }

    fn make_test_file(rel: &str, ext: &str, is_binary: bool, is_hidden: bool) -> SkillFile {
        SkillFile {
            relative_path: PathBuf::from(rel),
            absolute_path: PathBuf::from(format!("/tmp/test-skill/{}", rel)),
            file_type: SkillFileType::from_extension(ext),
            size_bytes: 100,
            is_binary,
            is_hidden,
        }
    }

    #[test]
    fn test_single_file_skips_structure_checks() {
        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_single_file("---\nname: test\n---\n", "/tmp/test.md", policy);
        let findings = validate(&ctx);
        assert!(
            findings.is_empty(),
            "SingleFile mode should return empty findings"
        );
    }

    #[test]
    fn test_valid_directory_no_findings() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // 创建一个有效名称的子目录
        let skill_dir = dir_path.join("valid-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();

        // 创建合法的 skill.md
        let skill_md = "---\nname: valid-skill\ndescription: A valid skill description for testing\nlicense: MIT\n---\n\nBody.";
        std::fs::write(skill_dir.join("skill.md"), skill_md).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(skill_dir.to_str().unwrap(), policy).unwrap();
        let findings = validate(&ctx);

        let non_info: Vec<_> = findings
            .iter()
            .filter(|f| !matches!(f.severity, IssueSeverity::Info))
            .collect();
        assert!(
            non_info.is_empty(),
            "Valid directory should have no findings, got: {:?}",
            non_info
        );
    }

    #[test]
    fn test_missing_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        // 不创建 skill.md
        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::new(ScanMode::Directory, Some(dir.path().to_path_buf()), policy);

        let findings = validate(&ctx);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_id, "STRUCTURE_MISSING_SKILL_MD");
        assert!(matches!(findings[0].severity, IssueSeverity::High));
    }

    #[test]
    fn test_invalid_name_format() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // 名称以下划线开头（不合法）
        let skill_md = "---\nname: _invalid-name\ndescription: A valid description here\n---\n";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = validate(&ctx);

        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "STRUCTURE_INVALID_NAME"),
            "Should detect invalid name, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_disallowed_extension() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: A valid description here\n---\n";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();

        // 创建一个不被允许的 .exe 文件
        std::fs::write(dir_path.join("malware.exe"), [0x00]).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = validate(&ctx);

        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "STRUCTURE_DISALLOWED_EXTENSION"),
            "Should detect disallowed extension, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_description_too_short() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\nname: test-skill\ndescription: short\n---\n";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = validate(&ctx);

        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "STRUCTURE_INVALID_DESCRIPTION"),
            "Should detect short description, got: {:?}",
            findings.iter().map(|f| &f.rule_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_is_valid_name_cases() {
        // 合法
        assert!(is_valid_name("ab"));
        assert!(is_valid_name("my-skill"));
        assert!(is_valid_name("skill123"));
        assert!(is_valid_name("a-b-c-d"));
        assert!(is_valid_name(&"a".repeat(64)));

        // 不合法
        assert!(!is_valid_name("a")); // 太短
        assert!(!is_valid_name(&"a".repeat(65))); // 太长
        assert!(!is_valid_name("-starts-hyphen")); // 以连字符开头
        assert!(!is_valid_name("ends-hyphen-")); // 以连字符结尾
        assert!(!is_valid_name("has--double")); // 连续连字符
        assert!(!is_valid_name("Has_Caps")); // 大写字母
        assert!(!is_valid_name("has space")); // 空格
        assert!(!is_valid_name("has.dot")); // 点号
    }

    #[test]
    fn test_hidden_file_detection() {
        let files = vec![make_test_file(".env", "env", false, true)];
        let mut ctx = make_test_ctx(ScanMode::Directory, files);
        // 需要 skill_md_path 和 manifest 才能走到文件检查
        ctx.skill_md_path = Some(PathBuf::from("/tmp/test-skill/skill.md"));
        ctx.manifest = Some(crate::security::skill_context::SkillManifest {
            name: "test-skill".to_string(),
            description: "A valid description here".to_string(),
            ..Default::default()
        });

        let findings = validate(&ctx);
        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "STRUCTURE_HIDDEN_FILE"),
            "Should detect hidden file"
        );
    }

    #[test]
    fn test_finding_has_stable_id() {
        let f1 = make_finding(
            "TEST_RULE",
            IssueSeverity::Medium,
            "desc".into(),
            Some("a.md".into()),
            Some(1),
        );
        let f2 = make_finding(
            "TEST_RULE",
            IssueSeverity::Medium,
            "desc".into(),
            Some("a.md".into()),
            Some(1),
        );
        let f3 = make_finding(
            "TEST_RULE",
            IssueSeverity::Medium,
            "desc".into(),
            Some("b.md".into()),
            Some(1),
        );
        assert_eq!(f1.id, f2.id, "Same inputs should produce same ID");
        assert_ne!(f1.id, f3.id, "Different file should produce different ID");
    }

    #[test]
    fn test_finding_category_is_policy_violation() {
        let f = make_finding(
            "STRUCTURE_MISSING_SKILL_MD",
            IssueSeverity::High,
            "test".into(),
            None,
            None,
        );
        assert!(
            matches!(f.category, ThreatCategory::PolicyViolation),
            "Structure findings should use PolicyViolation category"
        );
    }

    #[test]
    fn test_empty_name_not_flagged_as_invalid() {
        // 空 name 不应触发 STRUCTURE_INVALID_NAME（因为 frontmatter 中 name 默认为空）
        // 但 description 校验仍应触发
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let skill_md = "---\ndescription: A valid description here\n---\n";
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = validate(&ctx);

        // name 为空字符串时 is_valid_name 返回 false，但因为我们用 !name.is_empty() 做条件
        // 所以不会触发 INVALID_NAME
        assert!(
            !findings
                .iter()
                .any(|f| f.rule_id == "STRUCTURE_INVALID_NAME"),
            "Empty name should not trigger INVALID_NAME"
        );
    }

    #[test]
    fn test_description_too_long() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let long_desc = "x".repeat(1025);
        let skill_md = format!("---\nname: test-skill\ndescription: {}\n---\n", long_desc);
        std::fs::write(dir_path.join("skill.md"), skill_md).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let findings = validate(&ctx);

        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "STRUCTURE_INVALID_DESCRIPTION"),
            "Should detect too-long description"
        );
    }
}
