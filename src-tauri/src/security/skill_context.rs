//! SkillContext 数据模型
//!
//! 描述被扫描 Skill 的结构化上下文：清单元数据、文件清单、
//! 文件分类、以及引用的脚本/资产文件列表。
//!
//! 该模型在扫描流水线入口处构建，供所有扫描规则共用。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::security::policy::ScanPolicy;

/// 常见大目录（与 crate::security::SKIP_DIR_NAMES 共用）
use crate::security::SKIP_DIR_NAMES;

const SKIP_FILE_NAMES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

fn should_skip_context_dir(name: &str) -> bool {
    name != "__pycache__" && SKIP_DIR_NAMES.contains(&name)
}

// ── ScanMode ──

/// 扫描模式：单文件或整个目录
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    /// 扫描单个 Skill Markdown 文件
    SingleFile,
    /// 扫描整个 Skill 目录
    Directory,
}

// ── SkillFileType ──

/// Skill 中的文件类型分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillFileType {
    /// Markdown 文档（.md）
    Markdown,
    /// 脚本文件（.sh, .py, .js, .ts 等）
    Script,
    /// 配置文件（.json, .yaml, .yml, .toml, .env 等）
    Config,
    /// 静态资源（图片、字体等惰性文件）
    Asset,
    /// 二进制文件（无法以文本方式扫描）
    Binary,
    /// 未知类型
    Unknown,
}

impl SkillFileType {
    /// 根据文件扩展名推断文件类型
    pub fn from_extension(ext: &str) -> Self {
        let ext_lower = ext.to_lowercase();
        match ext_lower.as_str() {
            // Markdown / 文档
            "md" | "markdown" | "mdx" => SkillFileType::Markdown,

            // 脚本
            "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" | "py" | "pyw" | "rb" | "pl"
            | "php" | "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx" | "lua" | "r"
            | "rs" | "go" | "java" | "kt" | "kts" | "swift" | "dart" | "ex" | "exs" | "clj"
            | "cljs" | "hs" | "elm" | "v" | "zig" => SkillFileType::Script,

            // 配置
            "json" | "json5" | "jsonc" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf"
            | "env" | "properties" | "xml" | "xsd" | "xsl" | "xslt" | "gitignore"
            | "gitattributes" | "editorconfig" | "prettierrc" | "eslintrc" | "stylelintrc"
            | "dockerignore" | "npmrc" | "nvmrc" => SkillFileType::Config,

            // 静态资源（惰性扩展名）
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp" | "ico" | "icns" | "tif"
            | "tiff" | "heic" | "heif" | "svg" | "ttf" | "otf" | "woff" | "woff2" | "eot"
            | "mp3" | "mp4" | "wav" | "ogg" | "webm" | "flac" | "aac" | "pdf" | "doc" | "docx"
            | "xls" | "xlsx" | "ppt" | "pptx" | "zip" | "gz" | "tar" | "rar" | "7z" | "bz2"
            | "xz" | "pyc" | "pyo" | "class" | "o" | "so" | "dll" | "dylib" | "exe" | "msi"
            | "dmg" => SkillFileType::Asset,

            // 二进制可识别格式
            "bin" | "dat" | "db" | "sqlite" | "sqlite3" | "wasm" | "jar" | "war" | "ear" => {
                SkillFileType::Binary
            }

            _ => SkillFileType::Unknown,
        }
    }

    /// 从 Path 中提取文件类型
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map(SkillFileType::from_extension)
            .unwrap_or(SkillFileType::Unknown)
    }

    /// 是否为脚本文件类型
    pub fn is_script(&self) -> bool {
        matches!(self, SkillFileType::Script)
    }

    /// 是否为二进制文件类型（无法以文本方式扫描）
    pub fn is_binary_type(&self) -> bool {
        matches!(self, SkillFileType::Binary | SkillFileType::Asset)
    }
}

// ── SkillManifest ──

/// Skill 清单元数据（从 skill.md front-matter 或目录结构中提取）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Skill 名称
    #[serde(default)]
    pub name: String,
    /// Skill 描述
    #[serde(default)]
    pub description: String,
    /// 声明允许使用的工具列表（支持 YAML 中的 `allowed-tools` 或 `allowed_tools`）
    #[serde(default, alias = "allowed-tools")]
    pub allowed_tools: Vec<String>,
    /// 兼容性信息（目标平台、版本等）
    #[serde(default, alias = "compatibility-info")]
    pub compatibility: HashMap<String, String>,
    /// 许可证字段（可选）
    #[serde(default)]
    pub license: Option<String>,
    /// 其他元数据字段
    #[serde(default, alias = "meta")]
    pub metadata: HashMap<String, String>,
}

// ── SkillFile ──

/// Skill 目录中的单个文件信息
#[derive(Debug, Clone)]
pub struct SkillFile {
    /// 相对于 Skill 根目录的路径
    pub relative_path: PathBuf,
    /// 文件绝对路径
    pub absolute_path: PathBuf,
    /// 文件类型分类
    pub file_type: SkillFileType,
    /// 文件大小（字节）
    pub size_bytes: u64,
    /// 是否为二进制文件
    pub is_binary: bool,
    /// 是否为隐藏文件（以 . 开头）
    pub is_hidden: bool,
}

// ── SkillContext ──

/// Skill 扫描上下文：在扫描流水线入口处构建，包含扫描所需的所有信息
#[derive(Debug, Clone)]
pub struct SkillContext {
    /// 扫描模式
    pub scan_mode: ScanMode,
    /// Skill 根目录（SingleFile 模式下为 None）
    pub skill_dir: Option<PathBuf>,
    /// skill.md 文件路径（SingleFile 模式下即为扫描目标）
    pub skill_md_path: Option<PathBuf>,
    /// 解析后的清单元数据（None 表示未找到 front-matter）
    pub manifest: Option<SkillManifest>,
    /// skill.md 正文（去除 front-matter 后的指令体）
    pub instruction_body: Option<String>,
    /// 所有文件列表
    pub files: Vec<SkillFile>,
    /// 引用的文件（在 skill.md 中被提及的文件）
    pub referenced_files: Vec<PathBuf>,
    /// 脚本文件路径列表
    pub script_files: Vec<PathBuf>,
    /// 资产文件路径列表
    pub asset_files: Vec<PathBuf>,
    /// 扫描策略
    pub scan_policy: ScanPolicy,
    /// 内存中的文件内容（单文件扫描或无需落盘时使用，键为相对路径）
    pub file_contents: HashMap<String, String>,
}

impl SkillContext {
    /// 创建一个新的 SkillContext
    pub fn new(scan_mode: ScanMode, skill_dir: Option<PathBuf>, scan_policy: ScanPolicy) -> Self {
        Self {
            scan_mode,
            skill_dir,
            skill_md_path: None,
            manifest: None,
            instruction_body: None,
            files: Vec::new(),
            referenced_files: Vec::new(),
            script_files: Vec::new(),
            asset_files: Vec::new(),
            scan_policy,
            file_contents: HashMap::new(),
        }
    }

    /// 读取文件文本：优先使用 `file_contents`，否则从磁盘读取
    pub fn read_text_file(&self, file: &SkillFile) -> Option<String> {
        let rel = file.relative_path.to_string_lossy().to_string();
        if let Some(content) = self.file_contents.get(&rel) {
            return Some(content.clone());
        }
        if let Some(abs) = file.absolute_path.to_str() {
            if let Some(content) = self.file_contents.get(abs) {
                return Some(content.clone());
            }
        }
        std::fs::read_to_string(&file.absolute_path).ok()
    }

    /// 获取 Skill 名称（优先使用 manifest 中的名称，否则从目录名推断）
    pub fn skill_name(&self) -> String {
        if let Some(ref manifest) = self.manifest {
            if !manifest.name.is_empty() {
                return manifest.name.clone();
            }
        }
        self.skill_dir
            .as_ref()
            .and_then(|dir| dir.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// 获取文件总数
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// 获取所有文件的总大小（字节）
    pub fn total_size_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }

    /// 按类型过滤文件
    pub fn files_by_type(&self, file_type: SkillFileType) -> Vec<&SkillFile> {
        self.files
            .iter()
            .filter(|f| f.file_type == file_type)
            .collect()
    }

    /// 获取所有脚本文件的路径
    pub fn script_paths(&self) -> Vec<&Path> {
        self.script_files.iter().map(|p| p.as_path()).collect()
    }

    /// 获取所有资产文件的路径
    pub fn asset_paths(&self) -> Vec<&Path> {
        self.asset_files.iter().map(|p| p.as_path()).collect()
    }

    /// 检查某个文件是否在引用列表中
    pub fn is_referenced(&self, path: &Path) -> bool {
        self.referenced_files.iter().any(|p| p == path)
    }

    /// 从内容中解析 YAML frontmatter
    /// 返回 (manifest, instruction_body)
    /// 解析失败返回 (None, 原始内容)
    pub fn parse_frontmatter(content: &str) -> (Option<SkillManifest>, String) {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return (None, content.to_string());
        }
        let leading_len = content.len() - trimmed.len();
        let after_first = &trimmed[3..];
        let second = match after_first.find("\n---") {
            Some(pos) => pos,
            None => return (None, content.to_string()),
        };
        let yaml_str = &after_first[..second];
        let body_start = leading_len + 3 + second + 4; // "---\n" = 4 bytes
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

    /// 为单文件扫描构建上下文
    pub fn for_single_file(content: &str, file_path: &str, policy: ScanPolicy) -> Self {
        use std::path::Path;

        let path = Path::new(file_path);
        let file_type = SkillFileType::from_path(path);

        let (manifest, instruction_body) = if file_type == SkillFileType::Markdown {
            let (m, body) = Self::parse_frontmatter(content);
            (m, Some(body))
        } else {
            (None, Some(content.to_string()))
        };
        let mut files = Vec::new();
        let mut script_files = Vec::new();
        let mut file_contents = HashMap::new();
        // 同时以 file_path（可能是相对路径）和 path 显示名作为 key，
        // 确保 read_text_file 无论是用 relative_path 还是 absolute_path 查找都能命中
        file_contents.insert(file_path.to_string(), content.to_string());
        let path_display = path.to_string_lossy().to_string();
        if path_display != file_path {
            file_contents.insert(path_display, content.to_string());
        }

        if file_type == SkillFileType::Script {
            script_files.push(path.to_path_buf());
            files.push(SkillFile {
                relative_path: path.to_path_buf(),
                absolute_path: path.to_path_buf(),
                file_type,
                size_bytes: content.len() as u64,
                is_binary: false,
                is_hidden: false,
            });
        }

        Self {
            scan_mode: ScanMode::SingleFile,
            skill_dir: None,
            skill_md_path: Some(path.to_path_buf()),
            manifest,
            instruction_body,
            files,
            referenced_files: Vec::new(),
            script_files,
            asset_files: Vec::new(),
            scan_policy: policy,
            file_contents,
        }
    }

    /// 为目录扫描构建上下文
    pub fn for_directory(dir_path: &str, policy: ScanPolicy) -> anyhow::Result<Self> {
        use walkdir::WalkDir;

        let path = Path::new(dir_path);
        if !path.exists() || !path.is_dir() {
            anyhow::bail!("Directory does not exist: {}", dir_path);
        }

        let mut files = Vec::new();
        let mut skill_md_path = None;
        let mut script_files = Vec::new();
        let mut asset_files = Vec::new();

        let mut file_count = 0usize;
        let mut iter = WalkDir::new(path)
            .follow_links(false)
            .max_depth(policy.file_limits.max_depth)
            .into_iter();

        while let Some(next) = iter.next() {
            let entry = match next {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if should_skip_context_dir(name) {
                        iter.skip_current_dir();
                    }
                }
                continue;
            }

            if file_count >= policy.file_limits.max_files {
                break;
            }

            if let Some(name) = entry.file_name().to_str() {
                if SKIP_FILE_NAMES
                    .iter()
                    .any(|skip_name| name.eq_ignore_ascii_case(skip_name))
                {
                    continue;
                }
            }

            let abs_path = entry.path().to_path_buf();
            let rel_path = abs_path
                .strip_prefix(path)
                .unwrap_or(&abs_path)
                .to_string_lossy()
                .to_string();

            if entry
                .file_name()
                .to_str()
                .map_or(false, |n| n.eq_ignore_ascii_case("skill.md"))
            {
                skill_md_path = Some(abs_path.clone());
            }

            let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let file_type = SkillFileType::from_extension(ext);
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let is_hidden = rel_path.split('/').any(|seg| seg.starts_with('.'));

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
                relative_path: PathBuf::from(rel_path.clone()),
                absolute_path: abs_path,
                file_type,
                size_bytes: size,
                is_binary,
                is_hidden,
            };

            if file_type.is_script() {
                script_files.push(PathBuf::from(rel_path.clone()));
            }
            if matches!(file_type, SkillFileType::Asset | SkillFileType::Binary) {
                asset_files.push(PathBuf::from(rel_path.clone()));
            }
            files.push(skill_file);
            file_count += 1;
        }

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

        // 提取引用的文件
        let mut referenced_files = Vec::new();
        if let Some(ref body) = instruction_body {
            let refs = crate::security::referenced_files::extract_references(body, Some(path));
            for ref_path in refs {
                referenced_files.push(PathBuf::from(ref_path));
            }
        }

        Ok(Self {
            scan_mode: ScanMode::Directory,
            skill_dir: Some(path.to_path_buf()),
            skill_md_path,
            manifest,
            instruction_body,
            files,
            referenced_files,
            script_files,
            asset_files,
            scan_policy: policy,
            file_contents: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_type_from_extension_common() {
        assert_eq!(SkillFileType::from_extension("md"), SkillFileType::Markdown);
        assert_eq!(SkillFileType::from_extension("py"), SkillFileType::Script);
        assert_eq!(SkillFileType::from_extension("sh"), SkillFileType::Script);
        assert_eq!(SkillFileType::from_extension("js"), SkillFileType::Script);
        assert_eq!(SkillFileType::from_extension("ts"), SkillFileType::Script);
        assert_eq!(SkillFileType::from_extension("json"), SkillFileType::Config);
        assert_eq!(SkillFileType::from_extension("yaml"), SkillFileType::Config);
        assert_eq!(SkillFileType::from_extension("yml"), SkillFileType::Config);
        assert_eq!(SkillFileType::from_extension("png"), SkillFileType::Asset);
        assert_eq!(SkillFileType::from_extension("svg"), SkillFileType::Asset);
        assert_eq!(SkillFileType::from_extension("ttf"), SkillFileType::Asset);
        assert_eq!(SkillFileType::from_extension("bin"), SkillFileType::Binary);
        assert_eq!(SkillFileType::from_extension("wasm"), SkillFileType::Binary);
        assert_eq!(SkillFileType::from_extension("xyz"), SkillFileType::Unknown);
    }

    #[test]
    fn test_file_type_from_extension_case_insensitive() {
        assert_eq!(SkillFileType::from_extension("MD"), SkillFileType::Markdown);
        assert_eq!(SkillFileType::from_extension("Py"), SkillFileType::Script);
        assert_eq!(SkillFileType::from_extension("JSON"), SkillFileType::Config);
    }

    #[test]
    fn test_file_type_from_path() {
        assert_eq!(
            SkillFileType::from_path(Path::new("skill.md")),
            SkillFileType::Markdown
        );
        assert_eq!(
            SkillFileType::from_path(Path::new("scripts/run.sh")),
            SkillFileType::Script
        );
        assert_eq!(
            SkillFileType::from_path(Path::new("assets/logo.png")),
            SkillFileType::Asset
        );
    }

    #[test]
    fn test_skill_context_new() {
        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::new(
            ScanMode::Directory,
            Some(PathBuf::from("/tmp/test-skill")),
            policy,
        );
        assert_eq!(ctx.scan_mode, ScanMode::Directory);
        assert_eq!(ctx.skill_dir, Some(PathBuf::from("/tmp/test-skill")));
        assert_eq!(ctx.skill_name(), "test-skill");
        assert_eq!(ctx.file_count(), 0);
        assert_eq!(ctx.total_size_bytes(), 0);
    }

    #[test]
    fn test_skill_context_name_fallback() {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(
            ScanMode::SingleFile,
            Some(PathBuf::from("/tmp/my-skill")),
            policy,
        );
        // manifest name 为空时使用目录名
        assert_eq!(ctx.skill_name(), "my-skill");
        // manifest name 优先
        ctx.manifest = Some(SkillManifest {
            name: "custom-name".to_string(),
            ..Default::default()
        });
        assert_eq!(ctx.skill_name(), "custom-name");
    }

    #[test]
    fn test_skill_context_files_by_type() {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(
            ScanMode::Directory,
            Some(PathBuf::from("/tmp/skill")),
            policy,
        );

        ctx.files.push(SkillFile {
            relative_path: PathBuf::from("skill.md"),
            absolute_path: PathBuf::from("/tmp/skill/skill.md"),
            file_type: SkillFileType::Markdown,
            size_bytes: 100,
            is_binary: false,
            is_hidden: false,
        });
        ctx.files.push(SkillFile {
            relative_path: PathBuf::from("run.sh"),
            absolute_path: PathBuf::from("/tmp/skill/run.sh"),
            file_type: SkillFileType::Script,
            size_bytes: 200,
            is_binary: false,
            is_hidden: false,
        });

        assert_eq!(ctx.file_count(), 2);
        assert_eq!(ctx.total_size_bytes(), 300);
        assert_eq!(ctx.files_by_type(SkillFileType::Markdown).len(), 1);
        assert_eq!(ctx.files_by_type(SkillFileType::Script).len(), 1);
        assert_eq!(ctx.files_by_type(SkillFileType::Asset).len(), 0);
    }

    #[test]
    fn test_skill_context_is_referenced() {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(
            ScanMode::Directory,
            Some(PathBuf::from("/tmp/skill")),
            policy,
        );

        ctx.referenced_files
            .push(PathBuf::from("/tmp/skill/run.sh"));
        assert!(ctx.is_referenced(Path::new("/tmp/skill/run.sh")));
        assert!(!ctx.is_referenced(Path::new("/tmp/skill/unreferenced.txt")));
    }

    #[test]
    fn test_file_type_script_coverage() {
        // 验证常见脚本扩展名都覆盖
        let script_exts = vec![
            "sh", "bash", "py", "rb", "pl", "php", "js", "mjs", "ts", "mts", "tsx", "jsx", "lua",
            "r", "rs", "go", "java", "kt", "swift", "dart", "ex", "hs", "elm", "v", "zig",
        ];
        for ext in script_exts {
            assert_eq!(
                SkillFileType::from_extension(ext),
                SkillFileType::Script,
                "Extension '{}' should be Script",
                ext
            );
        }
    }

    #[test]
    fn test_file_type_config_coverage() {
        let config_exts = vec![
            "json",
            "json5",
            "yaml",
            "yml",
            "toml",
            "ini",
            "cfg",
            "env",
            "xml",
            "gitignore",
            "editorconfig",
        ];
        for ext in config_exts {
            assert_eq!(
                SkillFileType::from_extension(ext),
                SkillFileType::Config,
                "Extension '{}' should be Config",
                ext
            );
        }
    }

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\nname: my-skill\ndescription: A test skill\nallowed-tools:\n  - bash\n  - read\n---\n\nThis is the body.";
        let (manifest, body) = SkillContext::parse_frontmatter(content);
        let m = manifest.expect("should parse manifest");
        assert_eq!(m.name, "my-skill");
        assert_eq!(m.description, "A test skill");
        assert_eq!(m.allowed_tools, vec!["bash", "read"]);
        assert_eq!(body, "This is the body.");
    }

    #[test]
    fn test_parse_frontmatter_with_leading_whitespace_keeps_body() {
        let content = "  \n---\nname: my-skill\ndescription: A test skill\n---\n\nThis is the body.";
        let (manifest, body) = SkillContext::parse_frontmatter(content);

        manifest.expect("should parse manifest");
        assert_eq!(body, "This is the body.");
    }

    #[test]
    fn test_parse_frontmatter_missing() {
        let content = "This is just plain text without frontmatter.";
        let (manifest, body) = SkillContext::parse_frontmatter(content);
        assert!(manifest.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_parse_frontmatter_empty_fields() {
        let content = "---\nname: partial-skill\n---\n\nBody only.";
        let (manifest, body) = SkillContext::parse_frontmatter(content);
        let m = manifest.expect("should parse manifest");
        assert_eq!(m.name, "partial-skill");
        assert!(m.description.is_empty());
        assert!(m.allowed_tools.is_empty());
        assert_eq!(body, "Body only.");
    }

    #[test]
    fn test_parse_frontmatter_invalid_yaml() {
        let content = "---\n: invalid: yaml: [[[\n---\nBody.";
        let (manifest, body) = SkillContext::parse_frontmatter(content);
        assert!(manifest.is_none());
        assert_eq!(body, content);
    }

    // ── for_single_file tests ──

    #[test]
    fn test_for_single_file() {
        let content = "---\nname: test-skill\ndescription: A test\n---\n\nInstruction body here.";
        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_single_file(content, "/tmp/test.md", policy);

        assert_eq!(ctx.scan_mode, ScanMode::SingleFile);
        assert!(ctx.skill_dir.is_none());
        assert_eq!(ctx.skill_md_path, Some(PathBuf::from("/tmp/test.md")));
        let manifest = ctx.manifest.as_ref().expect("manifest should exist");
        assert_eq!(manifest.name, "test-skill");
        assert_eq!(manifest.description, "A test");
        assert_eq!(
            ctx.instruction_body.as_deref(),
            Some("Instruction body here.")
        );
        assert!(ctx.files.is_empty());
        assert!(ctx.script_files.is_empty());
        assert!(ctx.asset_files.is_empty());
    }

    #[test]
    fn test_for_single_file_no_frontmatter() {
        let content = "Just plain instructions, no frontmatter.";
        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_single_file(content, "/tmp/plain.md", policy);

        assert_eq!(ctx.scan_mode, ScanMode::SingleFile);
        assert!(ctx.manifest.is_none() || ctx.manifest.as_ref().unwrap().name.is_empty());
        assert_eq!(
            ctx.instruction_body.as_deref(),
            Some("Just plain instructions, no frontmatter.")
        );
    }

    // ── for_directory tests ──

    #[test]
    fn test_for_directory() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // 创建一个 skill.md
        let skill_md_content =
            "---\nname: dir-skill\nallowed-tools:\n  - bash\n---\n\nDo something.";
        std::fs::write(dir_path.join("skill.md"), skill_md_content).unwrap();

        // 创建一个脚本文件
        std::fs::write(dir_path.join("run.sh"), "#!/bin/bash\necho hello").unwrap();

        // 创建一个资产文件
        std::fs::write(dir_path.join("logo.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

        // 创建一个普通文件
        std::fs::write(dir_path.join("config.json"), "{}").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();

        assert_eq!(ctx.scan_mode, ScanMode::Directory);
        assert_eq!(ctx.skill_dir, Some(dir_path.to_path_buf()));
        assert!(ctx.skill_md_path.is_some());
        let md_path = ctx.skill_md_path.as_ref().unwrap();
        assert!(md_path.to_string_lossy().contains("skill.md"));

        let manifest = ctx.manifest.as_ref().expect("manifest should exist");
        assert_eq!(manifest.name, "dir-skill");
        assert_eq!(manifest.allowed_tools, vec!["bash"]);
        assert_eq!(ctx.instruction_body.as_deref(), Some("Do something."));

        // 应该发现 4 个文件
        assert_eq!(ctx.file_count(), 4);

        // 脚本文件应包含 run.sh
        assert_eq!(ctx.script_files.len(), 1);
        assert!(ctx.script_files[0].to_string_lossy().contains("run.sh"));

        // 资产文件应包含 logo.png
        assert_eq!(ctx.asset_files.len(), 1);
        assert!(ctx.asset_files[0].to_string_lossy().contains("logo.png"));
    }

    #[test]
    fn test_for_directory_missing() {
        let policy = ScanPolicy::builtin_default().clone();
        let result = SkillContext::for_directory("/nonexistent/path/that/does/not/exist", policy);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Directory does not exist"));
    }

    #[test]
    fn test_for_directory_empty() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();

        assert_eq!(ctx.scan_mode, ScanMode::Directory);
        assert_eq!(ctx.file_count(), 0);
        assert!(ctx.skill_md_path.is_none());
        assert!(ctx.manifest.is_none() || ctx.manifest.as_ref().unwrap().name.is_empty());
    }

    #[test]
    fn test_for_directory_with_hidden_files() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // 创建隐藏文件
        let hidden_dir = dir_path.join(".hidden");
        std::fs::create_dir(&hidden_dir).unwrap();
        std::fs::write(hidden_dir.join("secret.txt"), "secret").unwrap();

        // 创建正常文件
        std::fs::write(dir_path.join("visible.txt"), "visible").unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();

        // 两个文件都应该被发现
        assert_eq!(ctx.file_count(), 2);
        // 隐藏文件应该标记 is_hidden
        let hidden_file = ctx
            .files
            .iter()
            .find(|f| f.relative_path.to_string_lossy().contains("secret.txt"))
            .unwrap();
        assert!(hidden_file.is_hidden);
        let visible_file = ctx
            .files
            .iter()
            .find(|f| f.relative_path.to_string_lossy() == "visible.txt")
            .unwrap();
        assert!(!visible_file.is_hidden);
    }

    #[test]
    fn test_for_directory_skips_vcs_and_system_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        std::fs::write(dir_path.join("skill.md"), "Instructions").unwrap();
        std::fs::write(dir_path.join(".DS_Store"), [0u8, 1u8, 2u8]).unwrap();

        let git_dir = dir_path.join(".git").join("objects").join("pack");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(dir_path.join(".git").join("index"), [0u8, 1u8, 2u8]).unwrap();
        std::fs::write(git_dir.join("pack-test.idx"), [0u8, 1u8, 2u8]).unwrap();

        let cache_dir = dir_path.join("__pycache__");
        std::fs::create_dir(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("helper.pyc"), [0u8, 1u8, 2u8]).unwrap();

        let policy = ScanPolicy::builtin_default().clone();
        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();
        let mut paths: Vec<_> = ctx
            .files
            .iter()
            .map(|file| file.relative_path.to_string_lossy().replace('\\', "/"))
            .collect();
        paths.sort();

        assert_eq!(
            paths,
            vec!["__pycache__/helper.pyc", "skill.md"],
            "__pycache__ stays visible to structure analyzers while VCS/system metadata is skipped"
        );
    }

    #[test]
    fn test_for_directory_depth_limit() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path();

        // 创建嵌套目录结构
        let deep_dir = dir_path.join("a").join("b").join("c");
        std::fs::create_dir_all(&deep_dir).unwrap();
        std::fs::write(deep_dir.join("deep.txt"), "deep").unwrap();
        std::fs::write(dir_path.join("shallow.txt"), "shallow").unwrap();

        let mut policy = ScanPolicy::builtin_default().clone();
        policy.file_limits.max_depth = 2; // 限制深度为 2

        let ctx = SkillContext::for_directory(dir_path.to_str().unwrap(), policy).unwrap();

        // 只应发现 shallow.txt（深度 1），不应发现 a/b/c/deep.txt（深度 4）
        assert_eq!(ctx.file_count(), 1);
        assert_eq!(ctx.files[0].relative_path.to_string_lossy(), "shallow.txt");
    }

    #[test]
    fn test_is_script_method() {
        assert!(SkillFileType::Script.is_script());
        assert!(!SkillFileType::Markdown.is_script());
        assert!(!SkillFileType::Config.is_script());
        assert!(!SkillFileType::Asset.is_script());
        assert!(!SkillFileType::Binary.is_script());
        assert!(!SkillFileType::Unknown.is_script());
    }
}
