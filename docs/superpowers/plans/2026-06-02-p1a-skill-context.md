# P1a: SkillContext 构建、结构扫描与流程兼容 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立 Skill 语义解析基础，补齐结构扫描能力，确保现有流程不退化。

**Architecture:** 采用扁平模块结构，新增 `skill_context.rs`、`strict_structure.rs`、`referenced_files.rs`、`secret_masking.rs` 四个模块，重构 `scanner.rs` 为编排层。所有新模块通过 `SkillContext` 共享数据，`ScanMode` 控制分析器运行范围。

**Tech Stack:** Rust, serde_yaml, regex, walkdir, sha2

---

## 文件结构

| 文件 | 操作 | 职责 |
|------|------|------|
| `src-tauri/src/security/skill_context.rs` | 新建 | SkillContext、ScanMode、SkillManifest、SkillFile、frontmatter 解析 |
| `src-tauri/src/security/strict_structure.rs` | 新建 | 结构校验流水线 |
| `src-tauri/src/security/referenced_files.rs` | 新建 | 引用文件提取（6 种模式） |
| `src-tauri/src/security/secret_masking.rs` | 新建 | Secret 脱敏 |
| `src-tauri/src/security/scanner.rs` | 重构 | 编排层改造 |
| `src-tauri/src/security/mod.rs` | 修改 | 注册新模块 |

---

## Task 1: SkillContext 数据模型

**Files:**
- Create: `src-tauri/src/security/skill_context.rs`
- Modify: `src-tauri/src/security/mod.rs`

- [ ] **Step 1: 创建 skill_context.rs 基础结构**

```rust
// src-tauri/src/security/skill_context.rs
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::security::policy::ScanPolicy;

/// 扫描模式——控制哪些 analyzer 运行
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// 单文件扫描（scan_file 使用）：只有内容级规则运行
    SingleFile,
    /// 目录扫描（scan_directory_with_options 使用）：全部 analyzer 运行
    Directory,
}

/// Skill frontmatter 结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_yaml::Value>,
}

/// 目录中的文件信息
#[derive(Debug, Clone)]
pub struct SkillFile {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub file_type: SkillFileType,
    pub size_bytes: u64,
    pub is_binary: bool,
    pub is_hidden: bool,
}

/// 文件类型分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillFileType {
    Markdown,
    Script,
    Config,
    Asset,
    Binary,
    Unknown,
}

impl SkillFileType {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "md" => SkillFileType::Markdown,
            "py" | "pyw" | "pyi" | "sh" | "bash" | "zsh" | "ksh" | "fish" |
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" |
            "rb" | "rake" | "go" | "java" | "kt" | "cs" |
            "ps1" | "psm1" | "psd1" | "bat" | "cmd" | "php" => SkillFileType::Script,
            "json" | "yaml" | "yml" | "toml" | "cfg" | "ini" | "env" |
            "gitignore" | "gitattributes" | "editorconfig" => SkillFileType::Config,
            "svg" | "html" | "css" | "xml" | "xsd" => SkillFileType::Asset,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "ico" |
            "ttf" | "otf" | "woff" | "woff2" | "eot" | "pyc" | "pyo" |
            "db" | "sqlite" | "sqlite3" => SkillFileType::Binary,
            _ => SkillFileType::Unknown,
        }
    }

    pub fn is_script(&self) -> bool {
        matches!(self, SkillFileType::Script)
    }
}

/// Skill 语义上下文——所有 analyzer 共享的数据源
#[derive(Debug)]
pub struct SkillContext {
    pub scan_mode: ScanMode,
    pub skill_dir: Option<PathBuf>,
    pub skill_md_path: Option<PathBuf>,
    pub manifest: Option<SkillManifest>,
    pub instruction_body: Option<String>,
    pub files: Vec<SkillFile>,
    pub referenced_files: Vec<String>,
    pub script_files: Vec<String>,
    pub asset_files: Vec<String>,
    pub scan_policy: ScanPolicy,
}
```

- [ ] **Step 2: 在 mod.rs 注册新模块**

在 `src-tauri/src/security/mod.rs` 中添加：
```rust
pub mod skill_context;
```

- [ ] **Step 3: 验证编译**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo check
```

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/security/skill_context.rs src-tauri/src/security/mod.rs
git commit -m "feat(security): P1a Task 1 - SkillContext 数据模型"
```

---

## Task 2: Frontmatter 解析

**Files:**
- Modify: `src-tauri/src/security/skill_context.rs`

- [ ] **Step 1: 编写 frontmatter 解析测试**

在 `skill_context.rs` 底部添加测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = r#"---
name: my-skill
description: A test skill
allowed-tools:
  - Read
  - Write
---
# My Skill

This is the body."#;

        let (manifest, body) = SkillContext::parse_frontmatter(content);
        assert!(manifest.is_some());
        let m = manifest.unwrap();
        assert_eq!(m.name.as_deref(), Some("my-skill"));
        assert_eq!(m.description.as_deref(), Some("A test skill"));
        assert_eq!(m.allowed_tools.as_ref().unwrap().len(), 2);
        assert!(body.contains("This is the body."));
        assert!(!body.contains("---"));
    }

    #[test]
    fn test_parse_frontmatter_missing() {
        let content = "# No Frontmatter\n\nJust content.";
        let (manifest, body) = SkillContext::parse_frontmatter(content);
        assert!(manifest.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_frontmatter_empty_fields() {
        let content = r#"---
name: test
---
Body"#;

        let (manifest, _) = SkillContext::parse_frontmatter(content);
        let m = manifest.unwrap();
        assert_eq!(m.name.as_deref(), Some("test"));
        assert!(m.description.is_none());
        assert!(m.allowed_tools.is_none());
    }

    #[test]
    fn test_parse_frontmatter_invalid_yaml() {
        let content = r#"---
name: test
  invalid: yaml: structure
---
Body"#;

        // 解析失败应返回 None（不阻断扫描）
        let (manifest, _) = SkillContext::parse_frontmatter(content);
        assert!(manifest.is_none());
    }
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test skill_context::tests
```

- [ ] **Step 3: 实现 frontmatter 解析**

在 `SkillContext` impl 块中添加：

```rust
impl SkillContext {
    /// 从内容中解析 YAML frontmatter
    /// 返回 (manifest, instruction_body)
    /// 解析失败返回 (None, 原始内容)
    pub fn parse_frontmatter(content: &str) -> (Option<SkillManifest>, String) {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return (None, content.to_string());
        }

        // 查找第二个 ---
        let after_first = &trimmed[3..];
        let second = match after_first.find("\n---") {
            Some(pos) => pos,
            None => return (None, content.to_string()),
        };

        let yaml_str = &after_first[..second];
        let body_start = 3 + second + 4; // "---\n" = 4 bytes
        let body = if body_start < content.len() {
            content[body_start..].trim_start_matches('\n').to_string()
        } else {
            String::new()
        };

        match serde_yaml::from_str::<SkillManifest>(yaml_str) {
            Ok(manifest) => (Some(manifest), body),
            Err(_) => (None, content.to_string()),
        }
    }
}
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test skill_context::tests
```

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/security/skill_context.rs
git commit -m "feat(security): P1a Task 2 - frontmatter 解析"
```

---

## Task 3: SkillContext 构建器

**Files:**
- Modify: `src-tauri/src/security/skill_context.rs`

- [ ] **Step 1: 编写构建器测试**

```rust
#[test]
fn test_for_single_file() {
    let policy = ScanPolicy::default();
    let content = "---\nname: test\n---\nBody";
    let ctx = SkillContext::for_single_file(content, "test.md", policy);

    assert_eq!(ctx.scan_mode, ScanMode::SingleFile);
    assert!(ctx.skill_dir.is_none());
    assert!(ctx.manifest.is_some());
    assert_eq!(ctx.manifest.as_ref().unwrap().name.as_deref(), Some("test"));
    assert!(ctx.files.is_empty());
    assert!(ctx.referenced_files.is_empty());
}

#[test]
fn test_for_directory() {
    let dir = tempfile::tempdir().unwrap();
    let skill_md = dir.path().join("SKILL.md");
    std::fs::write(&skill_md, "---\nname: test\n---\nBody").unwrap();
    std::fs::write(dir.path().join("scripts/helper.py"), "print('hello')").unwrap();

    let policy = ScanPolicy::default();
    let ctx = SkillContext::for_directory(dir.path().to_str().unwrap(), policy).unwrap();

    assert_eq!(ctx.scan_mode, ScanMode::Directory);
    assert!(ctx.skill_dir.is_some());
    assert!(ctx.skill_md_path.is_some());
    assert!(ctx.manifest.is_some());
    assert!(!ctx.files.is_empty());
}

#[test]
fn test_for_directory_missing() {
    let policy = ScanPolicy::default();
    let result = SkillContext::for_directory("/nonexistent/path", policy);
    assert!(result.is_err());
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test skill_context::tests
```

- [ ] **Step 3: 实现构建器**

```rust
impl SkillContext {
    /// 为单文件扫描构建上下文
    pub fn for_single_file(content: &str, file_path: &str, policy: ScanPolicy) -> Self {
        let (manifest, instruction_body) = Self::parse_frontmatter(content);
        Self {
            scan_mode: ScanMode::SingleFile,
            skill_dir: None,
            skill_md_path: Some(PathBuf::from(file_path)),
            manifest,
            instruction_body: Some(instruction_body),
            files: Vec::new(),
            referenced_files: Vec::new(),
            script_files: Vec::new(),
            asset_files: Vec::new(),
            scan_policy: policy,
        }
    }

    /// 为目录扫描构建上下文
    pub fn for_directory(dir_path: &str, policy: ScanPolicy) -> anyhow::Result<Self> {
        use std::path::Path;
        use walkdir::WalkDir;

        let path = Path::new(dir_path);
        if !path.exists() || !path.is_dir() {
            anyhow::bail!("Directory does not exist: {}", dir_path);
        }

        let mut files = Vec::new();
        let mut skill_md_path = None;
        let mut script_files = Vec::new();
        let mut asset_files = Vec::new();

        // 遍历目录
        for entry in WalkDir::new(path)
            .follow_links(false)
            .max_depth(policy.file_limits.max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                continue;
            }

            let abs_path = entry.path().to_path_buf();
            let rel_path = abs_path
                .strip_prefix(path)
                .unwrap_or(&abs_path)
                .to_string_lossy()
                .to_string();

            // 检查是否为 SKILL.md
            if entry.file_name().to_str().map_or(false, |n| n.eq_ignore_ascii_case("skill.md")) {
                skill_md_path = Some(abs_path.clone());
            }

            let ext = abs_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let file_type = SkillFileType::from_extension(ext);
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

            // 检查是否为隐藏文件
            let is_hidden = rel_path.split('/').any(|seg| seg.starts_with('.'));

            // 简单二进制检测
            let is_binary = if let Ok(mut f) = std::fs::File::open(&abs_path) {
                let mut sample = [0u8; 512];
                if let Ok(n) = std::io::Read::read(&mut f, &mut sample) {
                    sample[..n].contains(&0u8)
                } else {
                    false
                }
            } else {
                false
            };

            let skill_file = SkillFile {
                relative_path: rel_path.clone(),
                absolute_path: abs_path,
                file_type,
                size_bytes: size,
                is_binary,
                is_hidden,
            };

            if file_type.is_script() {
                script_files.push(rel_path.clone());
            }
            if matches!(file_type, SkillFileType::Asset | SkillFileType::Binary) {
                asset_files.push(rel_path.clone());
            }

            files.push(skill_file);
        }

        // 解析 SKILL.md frontmatter
        let (manifest, instruction_body) = if let Some(ref md_path) = skill_md_path {
            match std::fs::read_to_string(md_path) {
                Ok(content) => {
                    let (m, b) = Self::parse_frontmatter(&content);
                    (m, Some(b))
                }
                Err(_) => (None, None),
            }
        } else {
            (None, None)
        };

        Ok(Self {
            scan_mode: ScanMode::Directory,
            skill_dir: Some(path.to_path_buf()),
            skill_md_path,
            manifest,
            instruction_body,
            files,
            referenced_files: Vec::new(), // 将在 Task 5 中填充
            script_files,
            asset_files,
            scan_policy: policy,
        })
    }
}
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test skill_context::tests
```

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/security/skill_context.rs
git commit -m "feat(security): P1a Task 3 - SkillContext 构建器"
```

---

## Task 4: 结构校验

**Files:**
- Create: `src-tauri/src/security/strict_structure.rs`
- Modify: `src-tauri/src/security/mod.rs`

- [ ] **Step 1: 创建 strict_structure.rs 并编写测试**

```rust
// src-tauri/src/security/strict_structure.rs
use crate::models::security::{Finding, FindingMetadata, IssueSeverity, ThreatCategory};
use crate::security::skill_context::{ScanMode, SkillContext};
use sha2::{Digest, Sha256};

/// 结构校验——检查 Skill 目录是否符合白名单规则
pub fn validate(ctx: &SkillContext) -> Vec<Finding> {
    // SingleFile 模式跳过所有结构检查
    if ctx.scan_mode == ScanMode::SingleFile {
        return Vec::new();
    }

    let mut findings = Vec::new();

    // 1. SKILL.md 存在性检查
    if ctx.skill_md_path.is_none() {
        findings.push(make_finding(
            "STRUCTURE_MISSING_SKILL_MD",
            IssueSeverity::High,
            "SKILL.md not found in skill directory",
            ctx.skill_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
            None,
        ));
        // SKILL.md 缺失时，后续 frontmatter 检查无意义
        return findings;
    }

    // 2. Frontmatter 校验
    if let Some(ref manifest) = ctx.manifest {
        // name 校验
        match &manifest.name {
            None => {
                findings.push(make_finding(
                    "STRUCTURE_INVALID_NAME",
                    IssueSeverity::Medium,
                    "Missing 'name' field in SKILL.md frontmatter",
                    None,
                    None,
                ));
            }
            Some(name) => {
                if !is_valid_name(name) {
                    findings.push(make_finding(
                        "STRUCTURE_INVALID_NAME",
                        IssueSeverity::Medium,
                        &format!("Invalid name format: '{}'. Must be lowercase alphanumeric with single hyphens, 2-64 chars", name),
                        None,
                        None,
                    ));
                }
            }
        }

        // description 校验
        match &manifest.description {
            None => {
                findings.push(make_finding(
                    "STRUCTURE_INVALID_DESCRIPTION",
                    IssueSeverity::Medium,
                    "Missing 'description' field in SKILL.md frontmatter",
                    None,
                    None,
                ));
            }
            Some(desc) => {
                if desc.len() < 10 {
                    findings.push(make_finding(
                        "STRUCTURE_INVALID_DESCRIPTION",
                        IssueSeverity::Medium,
                        &format!("Description too short ({} chars, minimum 10)", desc.len()),
                        None,
                        None,
                    ));
                } else if desc.len() > 1024 {
                    findings.push(make_finding(
                        "STRUCTURE_INVALID_DESCRIPTION",
                        IssueSeverity::Medium,
                        &format!("Description too long ({} chars, maximum 1024)", desc.len()),
                        None,
                        None,
                    ));
                }
            }
        }
    } else {
        // SKILL.md 存在但 frontmatter 解析失败
        findings.push(make_finding(
            "FRONTMATTER_PARSE_ERROR",
            IssueSeverity::Medium,
            "Failed to parse SKILL.md frontmatter",
            ctx.skill_md_path.as_ref().map(|p| p.to_string_lossy().to_string()),
            None,
        ));
    }

    // 3. 文件白名单检查
    let allowed_extensions = &ctx.scan_policy.strict_structure.allowed_extensions;
    let allowed_subdirs = &ctx.scan_policy.strict_structure.allowed_subdirs;

    for file in &ctx.files {
        let parts: Vec<&str> = file.relative_path.split('/').collect();

        // 隐藏文件检查
        if file.is_hidden {
            findings.push(make_finding(
                "STRUCTURE_HIDDEN_FILE",
                IssueSeverity::Medium,
                &format!("Hidden file/directory found: {}", file.relative_path),
                Some(file.relative_path.clone()),
                None,
            ));
        }

        // 顶层子目录检查（只检查直接在根目录下的目录）
        if parts.len() > 1 {
            let top_dir = parts[0];
            if !top_dir.contains('.') && !allowed_subdirs.contains(top_dir) {
                findings.push(make_finding(
                    "STRUCTURE_DISALLOWED_SUBDIR",
                    IssueSeverity::Medium,
                    &format!("Disallowed top-level subdirectory: {}", top_dir),
                    Some(file.relative_path.clone()),
                    None,
                ));
            }
        }

        // 扩展名检查
        let ext = std::path::Path::new(&file.relative_path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_default();

        // 无扩展名的文件（如 Makefile, LICENSE）允许通过
        if !ext.is_empty() && !allowed_extensions.contains(&ext) {
            findings.push(make_finding(
                "STRUCTURE_DISALLOWED_EXTENSION",
                IssueSeverity::Medium,
                &format!("Disallowed file extension: {} ({})", ext, file.relative_path),
                Some(file.relative_path.clone()),
                None,
            ));
        }

        // 二进制内容检查（非已知二进制扩展名的文件）
        if file.is_binary && !is_known_binary_ext(&ext) {
            findings.push(make_finding(
                "STRUCTURE_BINARY_CONTENT",
                IssueSeverity::Low,
                &format!("Binary content detected in text file: {}", file.relative_path),
                Some(file.relative_path.clone()),
                None,
            ));
        }
    }

    findings
}

/// 检查 name 格式：小写字母数字，单连字符，长度 2-64
fn is_valid_name(name: &str) -> bool {
    if name.len() < 2 || name.len() > 64 {
        return false;
    }
    let bytes = name.as_bytes();
    // 不能以连字符开头或结尾
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    // 不能有连续连字符
    for i in 0..bytes.len() {
        match bytes[i] {
            b'a'..=b'z' | b'0'..=b'9' => {}
            b'-' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// 已知二进制扩展名（不需要内容检查）
fn is_known_binary_ext(ext: &str) -> bool {
    matches!(
        ext,
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" | ".avif" | ".bmp" | ".ico" |
        ".ttf" | ".otf" | ".woff" | ".woff2" | ".eot" |
        ".pyc" | ".pyo" | ".db" | ".sqlite" | ".sqlite3" |
        ".zip" | ".tar" | ".gz" | ".bz2" | ".xz"
    )
}

/// 创建 Finding 的辅助函数
fn make_finding(
    rule_id: &str,
    severity: IssueSeverity,
    description: &str,
    file_path: Option<String>,
    line_number: Option<usize>,
) -> Finding {
    let mut hasher = Sha256::new();
    hasher.update(rule_id.as_bytes());
    hasher.update(description.as_bytes());
    if let Some(ref fp) = file_path {
        hasher.update(fp.as_bytes());
    }
    let id = format!("{:x}", hasher.finalize())[..16].to_string();

    Finding {
        id,
        rule_id: rule_id.to_string(),
        category: ThreatCategory::PolicyViolation,
        severity,
        title: rule_id.replace('_', " "),
        description: description.to_string(),
        file_path,
        line_number,
        snippet: None,
        remediation: None,
        analyzer: "strict_structure".to_string(),
        metadata: Some(FindingMetadata {
            ..Default::default()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::policy::ScanPolicy;
    use crate::security::skill_context::SkillFileType;
    use tempfile::tempdir;

    fn make_ctx_for_file(content: &str) -> SkillContext {
        SkillContext::for_single_file(content, "SKILL.md", ScanPolicy::default())
    }

    #[test]
    fn test_single_file_skips_structure_checks() {
        let ctx = make_ctx_for_file("no frontmatter");
        let findings = validate(&ctx);
        assert!(findings.is_empty(), "SingleFile mode should skip structure checks");
    }

    #[test]
    fn test_valid_directory_no_findings() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: my-skill\ndescription: A valid test skill\n---\nBody",
        ).unwrap();
        std::fs::write(dir.path().join("scripts/helper.py"), "print('hi')").unwrap();

        let ctx = SkillContext::for_directory(dir.path().to_str().unwrap(), ScanPolicy::default()).unwrap();
        let findings = validate(&ctx);
        assert!(findings.is_empty(), "Valid directory should have no findings, got: {:?}", findings);
    }

    #[test]
    fn test_missing_skill_md() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), "# Not a skill").unwrap();

        let ctx = SkillContext::for_directory(dir.path().to_str().unwrap(), ScanPolicy::default()).unwrap();
        let findings = validate(&ctx);
        assert!(findings.iter().any(|f| f.rule_id == "STRUCTURE_MISSING_SKILL_MD"));
    }

    #[test]
    fn test_invalid_name_format() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: Invalid_Name\ndescription: A test skill\n---\nBody",
        ).unwrap();

        let ctx = SkillContext::for_directory(dir.path().to_str().unwrap(), ScanPolicy::default()).unwrap();
        let findings = validate(&ctx);
        assert!(findings.iter().any(|f| f.rule_id == "STRUCTURE_INVALID_NAME"));
    }

    #[test]
    fn test_disallowed_extension() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\nBody",
        ).unwrap();
        std::fs::write(dir.path().join("malware.exe"), "MZ...").unwrap();

        let ctx = SkillContext::for_directory(dir.path().to_str().unwrap(), ScanPolicy::default()).unwrap();
        let findings = validate(&ctx);
        assert!(findings.iter().any(|f| f.rule_id == "STRUCTURE_DISALLOWED_EXTENSION"));
    }

    #[test]
    fn test_description_too_short() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("SKILL.md"),
            "---\nname: my-skill\ndescription: Short\n---\nBody",
        ).unwrap();

        let ctx = SkillContext::for_directory(dir.path().to_str().unwrap(), ScanPolicy::default()).unwrap();
        let findings = validate(&ctx);
        assert!(findings.iter().any(|f| f.rule_id == "STRUCTURE_INVALID_DESCRIPTION"));
    }
}
```

- [ ] **Step 2: 在 mod.rs 注册模块**

```rust
pub mod strict_structure;
```

- [ ] **Step 3: 运行测试验证失败**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test strict_structure::tests
```

- [ ] **Step 4: 运行测试验证通过**

（实现已在 Step 1 中包含，运行测试应直接通过）

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test strict_structure::tests
```

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/security/strict_structure.rs src-tauri/src/security/mod.rs
git commit -m "feat(security): P1a Task 4 - 结构校验"
```

---

## Task 5: 引用文件提取

**Files:**
- Create: `src-tauri/src/security/referenced_files.rs`
- Modify: `src-tauri/src/security/mod.rs`

- [ ] **Step 1: 创建 referenced_files.rs 并编写测试**

```rust
// src-tauri/src/security/referenced_files.rs
use regex::Regex;
use lazy_static::lazy_static;

/// Python 标准库模块列表（Python 3.9+）
const STDLIB_MODULES: &[&str] = &[
    "abc", "aifc", "argparse", "array", "ast", "asynchat", "asyncio", "asyncore",
    "atexit", "base64", "bdb", "binascii", "binhex", "bisect", "builtins",
    "bz2", "calendar", "cgi", "cgitb", "chunk", "cmath", "cmd", "code",
    "codecs", "codeop", "collections", "colorsys", "compileall", "concurrent",
    "configparser", "contextlib", "contextvars", "copy", "copyreg", "cProfile",
    "crypt", "csv", "ctypes", "curses", "dataclasses", "datetime", "dbm",
    "decimal", "difflib", "dis", "distutils", "doctest", "email", "encodings",
    "enum", "errno", "faulthandler", "fcntl", "filecmp", "fileinput", "fnmatch",
    "fractions", "ftplib", "functools", "gc", "getopt", "getpass", "gettext",
    "glob", "grp", "gzip", "hashlib", "heapq", "hmac", "html", "http",
    "idlelib", "imaplib", "imghdr", "imp", "importlib", "inspect", "io",
    "ipaddress", "itertools", "json", "keyword", "lib2to3", "linecache",
    "locale", "logging", "lzma", "mailbox", "mailcap", "marshal", "math",
    "mimetypes", "mmap", "modulefinder", "multiprocessing", "netrc", "nis",
    "nntplib", "numbers", "operator", "optparse", "os", "ossaudiodev",
    "pathlib", "pdb", "pickle", "pickletools", "pipes", "pkgutil", "platform",
    "plistlib", "poplib", "posix", "posixpath", "pprint", "profile", "pstats",
    "pty", "pwd", "py_compile", "pyclbr", "pydoc", "queue", "quopri",
    "random", "re", "readline", "reprlib", "resource", "rlcompleter", "runpy",
    "sched", "secrets", "select", "selectors", "shelve", "shlex", "shutil",
    "signal", "site", "smtpd", "smtplib", "sndhdr", "socket", "socketserver",
    "sqlite3", "ssl", "stat", "statistics", "string", "stringprep", "struct",
    "subprocess", "sunau", "symtable", "sys", "sysconfig", "syslog",
    "tabnanny", "tarfile", "telnetlib", "tempfile", "termios", "test",
    "textwrap", "threading", "time", "timeit", "tkinter", "token", "tokenize",
    "tomllib", "trace", "traceback", "tracemalloc", "tty", "turtle",
    "turtledemo", "types", "typing", "unicodedata", "unittest", "urllib",
    "uu", "uuid", "venv", "warnings", "wave", "weakref", "webbrowser",
    "winreg", "winsound", "wsgiref", "xdrlib", "xml", "xmlrpc", "zipapp",
    "zipfile", "zipimport", "zlib",
    // 常用子模块
    "os.path", "os.path", "json", "collections", "typing", "pathlib",
    "urllib.request", "urllib.parse", "urllib.error", "http.client",
    "email.mime", "email.mime.text", "email.mime.multipart",
];

/// 常见第三方包
const KNOWN_THIRD_PARTY: &[&str] = &[
    "requests", "flask", "django", "fastapi", "uvicorn", "starlette",
    "numpy", "pandas", "scipy", "matplotlib", "seaborn", "plotly",
    "sqlalchemy", "alembic", "pydantic", "attrs", "cattrs",
    "pytest", "unittest", "mock", "coverage", "tox", "nox",
    "click", "typer", "rich", "colorama", "tqdm",
    "boto3", "botocore", "google-cloud", "azure",
    "celery", "redis", "kombu",
    "pillow", "opencv", "scikit-learn", "torch", "tensorflow",
    "beautifulsoup4", "lxml", "scrapy", "selenium",
    "pyyaml", "toml", "tomli", "configparser",
    "jinja2", "mako", "markupsafe",
    "werkzeug", "gunicorn", "uvloop",
    "aiohttp", "httpx", "websockets",
    "pygments", "black", "isort", "flake8", "mypy", "pylint", "ruff",
    "setuptools", "pip", "wheel", "poetry", "pdm", "hatch",
    "twine", "build",
    "cryptography", "paramiko", "pycryptodome",
    "websocket-client", "slack-sdk", "discord.py",
    "pyserial", "paho-mqtt",
    "pyinstaller", "cx-freeze", "nuitka",
];

lazy_static! {
    static ref MD_LINK_RE: Regex =
        Regex::new(r"\[.*?\]\(([^)]+)\)").unwrap();
    static ref NATURAL_LANG_RE: Regex =
        Regex::new(r"(?i)(?:see|refer to|check|read)\s+[`\"']?(\S+\.\w+)[`\"']?").unwrap();
    static ref EXEC_REF_RE: Regex =
        Regex::new(r"(?i)(?:run|execute|invoke)\s+(scripts/\S+)").unwrap();
    static ref AT_REFERENCE_RE: Regex =
        Regex::new(r"@reference:\s*(.+)").unwrap();
    static ref INCLUDE_RE: Regex =
        Regex::new(r"(?i)(?:include|import|load):\s*(.+)").unwrap();
    static ref PYTHON_IMPORT_RE: Regex =
        Regex::new(r"(?m)^(?:from\s+(\S+)\s+)?import\s+(\S+)").unwrap();
}

/// 从内容中提取引用文件路径
pub fn extract_references(content: &str, skill_dir: Option<&std::path::Path>) -> Vec<String> {
    let mut refs = Vec::new();

    // 1. Markdown 链接
    for cap in MD_LINK_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            refs.push(path.to_string());
        }
    }

    // 2. 自然语言引用
    for cap in NATURAL_LANG_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            refs.push(path.to_string());
        }
    }

    // 3. 执行型引用
    for cap in EXEC_REF_RE.captures_iter(content) {
        refs.push(cap[1].trim().to_string());
    }

    // 4. @reference: 指令
    for cap in AT_REFERENCE_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            refs.push(path.to_string());
        }
    }

    // 5. include/import/load: 指令
    for cap in INCLUDE_RE.captures_iter(content) {
        let path = cap[1].trim();
        if is_valid_file_ref(path) {
            refs.push(path.to_string());
        }
    }

    // 6. Python import
    for cap in PYTHON_IMPORT_RE.captures_iter(content) {
        let module = if let Some(from_module) = cap.get(1) {
            from_module.as_str().trim()
        } else {
            cap[2].trim()
        };

        // 跳过标准库和已知第三方
        if STDLIB_MODULES.contains(&module) || KNOWN_THIRD_PARTY.contains(&module) {
            continue;
        }

        // 跳过相对导入
        if module.starts_with('.') {
            continue;
        }

        // 检查是否为本地模块
        if let Some(dir) = skill_dir {
            let module_path = module.replace('.', "/");
            let py_path = dir.join(format!("{}.py", module_path));
            let mod_path = dir.join(format!("{}/__init__.py", module_path));
            if py_path.exists() || mod_path.exists() {
                refs.push(format!("{}.py", module_path));
            }
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

/// 检查是否为有效的文件引用（排除 URL、锚点、绝对路径）
fn is_valid_file_ref(path: &str) -> bool {
    // 排除 URL
    if path.starts_with("http://") || path.starts_with("https://") || path.starts_with("ftp://") {
        return false;
    }
    // 排除锚点
    if path.starts_with('#') {
        return false;
    }
    // 排除绝对路径
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    // 排除路径穿越
    if path.contains("..") {
        return false;
    }
    // 必须包含扩展名
    path.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_links() {
        let content = "See [this file](scripts/helper.py) for details.";
        let refs = extract_references(content, None);
        assert!(refs.contains(&"scripts/helper.py".to_string()));
    }

    #[test]
    fn test_markdown_links_exclude_urls() {
        let content = "Visit [Google](https://google.com) for info.";
        let refs = extract_references(content, None);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_natural_language_reference() {
        let content = "Please check `config.json` for settings.";
        let refs = extract_references(content, None);
        assert!(refs.contains(&"config.json".to_string()));
    }

    #[test]
    fn test_execution_reference() {
        let content = "Run scripts/setup.py to initialize.";
        let refs = extract_references(content, None);
        assert!(refs.contains(&"scripts/setup.py".to_string()));
    }

    #[test]
    fn test_at_reference() {
        let content = "@reference: templates/base.md";
        let refs = extract_references(content, None);
        assert!(refs.contains(&"templates/base.md".to_string()));
    }

    #[test]
    fn test_include_directive() {
        let content = "include: config/settings.yaml";
        let refs = extract_references(content, None);
        assert!(refs.contains(&"config/settings.yaml".to_string()));
    }

    #[test]
    fn test_python_import_stdlib_excluded() {
        let content = "import os\nimport json\nfrom pathlib import Path";
        let refs = extract_references(content, None);
        // stdlib 应被排除
        assert!(refs.is_empty());
    }

    #[test]
    fn test_python_import_local_module() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("my_helper.py"), "def helper(): pass").unwrap();

        let content = "import my_helper\nmy_helper.helper()";
        let refs = extract_references(content, Some(dir.path()));
        assert!(refs.contains(&"my_helper.py".to_string()));
    }

    #[test]
    fn test_path_traversal_excluded() {
        let content = "See [hack](../../../etc/passwd)";
        let refs = extract_references(content, None);
        assert!(refs.is_empty());
    }
}
```

- [ ] **Step 2: 在 mod.rs 注册模块**

```rust
pub mod referenced_files;
```

- [ ] **Step 3: 运行测试**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test referenced_files::tests
```

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/security/referenced_files.rs src-tauri/src/security/mod.rs
git commit -m "feat(security): P1a Task 5 - 引用文件提取"
```

---

## Task 6: Secret 脱敏

**Files:**
- Create: `src-tauri/src/security/secret_masking.rs`
- Modify: `src-tauri/src/security/mod.rs`

- [ ] **Step 1: 创建 secret_masking.rs 并编写测试**

```rust
// src-tauri/src/security/secret_masking.rs
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    static ref AWS_KEY_RE: Regex =
        Regex::new(r"(AKIA|ASIA)[A-Z0-9]{16}").unwrap();
    static ref GITHUB_TOKEN_RE: Regex =
        Regex::new(r"(gh[opusr]_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{36,})").unwrap();
    static ref PRIVATE_KEY_RE: Regex =
        Regex::new(r"-----BEGIN\s+(?:RSA|OPENSSH|EC|DSA)?\s*PRIVATE KEY-----[\s\S]*?-----END\s+(?:RSA|OPENSSH|EC|DSA)?\s*PRIVATE KEY-----").unwrap();
    static ref JWT_RE: Regex =
        Regex::new(r"eyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}").unwrap();
    static ref DB_CONN_RE: Regex =
        Regex::new(r"(mongodb|mysql|postgresql|postgres)://[^\s"']{10,}").unwrap();
    static ref GENERIC_SECRET_RE: Regex =
        Regex::new(r#"(?:secret|token|key)\s*[=:]\s*["']([a-zA-Z0-9_-]{16,})["']"#).unwrap();
}

/// 对代码片段中的 secret 进行脱敏
pub fn mask_secrets(snippet: &str) -> String {
    let mut result = snippet.to_string();

    // AWS Key: 保留前缀 + 后 4 位
    result = AWS_KEY_RE.replace_all(&result, |caps: &regex::Captures| {
        let prefix = &caps[1];
        let full = caps[0].as_str();
        let last4 = &full[full.len() - 4..];
        format!("{}...{}", prefix, last4)
    }).to_string();

    // GitHub Token: 保留前缀 + 后 4 位
    result = GITHUB_TOKEN_RE.replace_all(&result, |caps: &regex::Captures| {
        let full = caps[0].as_str();
        let last4 = &full[full.len() - 4..];
        format!("{}...{}", &full[..8], last4)
    }).to_string();

    // 私钥: 只保留类型说明
    result = PRIVATE_KEY_RE.replace_all(&result, "-----BEGIN PRIVATE KEY----- [REDACTED] -----END PRIVATE KEY-----").to_string();

    // JWT: 保留 eyj 前缀
    result = JWT_RE.replace_all(&result, |caps: &regex::Captures| {
        let full = caps[0].as_str();
        format!("{}...[REDACTED]", &full[..10])
    }).to_string();

    // DB 连接串: 保留协议
    result = DB_CONN_RE.replace_all(&result, |caps: &regex::Captures| {
        let protocol = &caps[1];
        format!("{}://[REDACTED]", protocol)
    }).to_string();

    // 通用 token: 保留前 4 位
    result = GENERIC_SECRET_RE.replace_all(&result, |caps: &regex::Captures| {
        let key = &caps[1];
        let prefix = &key[..4.min(key.len())];
        format!("\"{}...\"", prefix)
    }).to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_aws_key() {
        let input = "api_key = 'AKIAIOSFODNN7EXAMPLE'";
        let masked = mask_secrets(input);
        assert!(masked.contains("AKIA...E"));
        assert!(!masked.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_mask_github_token() {
        let input = "token = 'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij'";
        let masked = mask_secrets(input);
        assert!(!masked.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
        assert!(masked.contains("ghp_ABCD"));
    }

    #[test]
    fn test_mask_private_key() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...\n-----END RSA PRIVATE KEY-----";
        let masked = mask_secrets(input);
        assert!(masked.contains("[REDACTED]"));
        assert!(!masked.contains("MIIEpAIBAAK"));
    }

    #[test]
    fn test_mask_jwt() {
        let input = "token = 'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U'";
        let masked = mask_secrets(input);
        assert!(masked.contains("[REDACTED]"));
        assert!(!masked.contains("dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"));
    }

    #[test]
    fn test_mask_db_connection_string() {
        let input = "db = 'postgresql://user:password@host:5432/dbname'";
        let masked = mask_secrets(input);
        assert!(masked.contains("postgresql://[REDACTED]"));
        assert!(!masked.contains("user:password@host"));
    }

    #[test]
    fn test_no_masking_on_normal_text() {
        let input = "This is normal code with no secrets.";
        let masked = mask_secrets(input);
        assert_eq!(masked, input);
    }
}
```

- [ ] **Step 2: 在 mod.rs 注册模块**

```rust
pub mod secret_masking;
```

- [ ] **Step 3: 运行测试**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test secret_masking::tests
```

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/security/secret_masking.rs src-tauri/src/security/mod.rs
git commit -m "feat(security): P1a Task 6 - Secret 脱敏"
```

---

## Task 7: Scanner 重构 — scan_file

**Files:**
- Modify: `src-tauri/src/security/scanner.rs`

- [ ] **Step 1: 编写 scan_file 的 SkillContext 集成测试**

在 `scanner.rs` 的 tests 模块中添加：

```rust
#[test]
fn test_scan_file_with_skill_context_does_not_produce_structure_false_positives() {
    let scanner = SecurityScanner::new();
    let content = "---\nname: my-skill\ndescription: A test skill\n---\n# Body\nNo dangerous code here.";
    let report = scanner.scan_file(content, "SKILL.md", "en").unwrap();

    // SingleFile 模式不应产生结构类误报
    assert!(!report.blocked);
    assert!(report.hard_trigger_issues.is_empty());
    assert!(!report.partial_scan);
    assert!(report.skipped_files.is_empty());

    // 不应有 STRUCTURE_ 前缀的 issue
    let structure_issues: Vec<_> = report.issues.iter()
        .filter(|i| i.rule_id.as_deref().map_or(false, |id| id.starts_with("STRUCTURE_")))
        .collect();
    assert!(structure_issues.is_empty(),
        "SingleFile scan should not produce structure findings, got: {:?}", structure_issues);
}
```

- [ ] **Step 2: 运行测试验证通过**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test test_scan_file_with_skill_context
```

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/security/scanner.rs
git commit -m "test(security): P1a Task 7 - scan_file SkillContext 集成测试"
```

---

## Task 8: Scanner 重构 — scan_directory_with_options

**Files:**
- Modify: `src-tauri/src/security/scanner.rs`

- [ ] **Step 1: 编写目录扫描的结构校验集成测试**

```rust
#[test]
fn test_scan_directory_includes_structure_validation() {
    let dir = tempfile::tempdir().unwrap();
    // 创建一个有结构问题的 skill 目录
    std::fs::write(
        dir.path().join("SKILL.md"),
        "---\nname: test-skill\ndescription: A valid test skill for testing\n---\nBody",
    ).unwrap();
    std::fs::write(dir.path().join("malware.exe"), "MZ...").unwrap();

    let scanner = SecurityScanner::new();
    let report = scanner.scan_directory(dir.path().to_str().unwrap(), "test", "en").unwrap();

    // 应该有结构校验的 issue
    let structure_issues: Vec<_> = report.issues.iter()
        .filter(|i| i.rule_id.as_deref().map_or(false, |id| id.starts_with("STRUCTURE_")))
        .collect();
    assert!(!structure_issues.is_empty(),
        "Directory scan should detect structure issues");
}

#[test]
fn test_scan_directory_valid_skill_no_structure_issues() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("SKILL.md"),
        "---\nname: my-skill\ndescription: A valid test skill for testing\n---\nBody",
    ).unwrap();
    std::fs::write(dir.path().join("scripts/helper.py"), "print('hi')").unwrap();

    let scanner = SecurityScanner::new();
    let report = scanner.scan_directory(dir.path().to_str().unwrap(), "test", "en").unwrap();

    let structure_issues: Vec<_> = report.issues.iter()
        .filter(|i| i.rule_id.as_deref().map_or(false, |id| id.starts_with("STRUCTURE_")))
        .collect();
    assert!(structure_issues.is_empty(),
        "Valid skill should have no structure issues, got: {:?}", structure_issues);
}
```

- [ ] **Step 2: 运行测试验证**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test test_scan_directory_includes_structure_validation
```

- [ ] **Step 3: 重构 scan_directory_with_options 使用 SkillContext**

在 `scan_directory_with_options` 函数开头添加 SkillContext 构建和结构校验：

```rust
// 在函数开头（现有 path 检查之后）添加：
use crate::security::skill_context::SkillContext;
use crate::security::strict_structure;

// 构建 SkillContext
let policy = crate::security::policy::ScanPolicy::default();
let skill_ctx = SkillContext::for_directory(dir_path, policy)?;

// 运行结构校验（仅 Directory 模式）
let structure_findings = strict_structure::validate(&skill_ctx);

// 将结构校验 findings 转换为 SecurityIssue 并添加到 all_issues
for finding in &structure_findings {
    all_issues.push(SecurityIssue {
        severity: finding.severity,
        category: crate::models::security::IssueCategory::Other,
        description: finding.description.clone(),
        line_number: finding.line_number,
        code_snippet: finding.snippet.clone(),
        file_path: finding.file_path.clone(),
        rule_id: Some(finding.rule_id.clone()),
        confidence: None,
        remediation: finding.remediation.clone(),
        cwe_id: finding.metadata.as_ref().and_then(|m| m.cwe_id.clone()),
    });
    // 结构校验的 Critical finding 触发 blocked
    if finding.severity == IssueSeverity::Critical {
        blocked = true;
        total_hard_trigger_issues.push(format!("{}: {}", finding.rule_id, finding.description));
    }
}
```

同时将目录遍历中的文件信息填充到 `skill_ctx.files`，以便后续 analyzer 使用。

- [ ] **Step 4: 运行全部测试**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test security
```

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/security/scanner.rs
git commit -m "refactor(security): P1a Task 8 - scanner 重构使用 SkillContext"
```

---

## Task 9: 回归测试与最终验证

- [ ] **Step 1: 运行全部测试**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard\src-tauri" && cargo test
```

- [ ] **Step 2: 验证前端编译**

```bash
cd "c:\Users\Bruce\VSCodeProject\agent-skills-guard" && pnpm build
```

- [ ] **Step 3: 最终提交**

```bash
git add -A
git commit -m "feat(security): P1a 完成 - SkillContext、结构扫描、引用提取、Secret 脱敏"
```
