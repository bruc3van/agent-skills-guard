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
            "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd"
            | "py" | "pyw" | "rb" | "pl" | "php"
            | "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx"
            | "lua" | "r" | "rs" | "go" | "java" | "kt" | "kts"
            | "swift" | "dart" | "ex" | "exs" | "clj" | "cljs"
            | "hs" | "elm" | "v" | "zig" => SkillFileType::Script,

            // 配置
            "json" | "json5" | "jsonc"
            | "yaml" | "yml"
            | "toml" | "ini" | "cfg" | "conf"
            | "env" | "properties"
            | "xml" | "xsd" | "xsl" | "xslt"
            | "gitignore" | "gitattributes" | "editorconfig"
            | "prettierrc" | "eslintrc" | "stylelintrc"
            | "dockerignore" | "npmrc" | "nvmrc" => SkillFileType::Config,

            // 静态资源（惰性扩展名）
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "bmp"
            | "ico" | "icns" | "tif" | "tiff" | "heic" | "heif"
            | "svg"
            | "ttf" | "otf" | "woff" | "woff2" | "eot"
            | "mp3" | "mp4" | "wav" | "ogg" | "webm" | "flac" | "aac"
            | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
            | "zip" | "gz" | "tar" | "rar" | "7z" | "bz2" | "xz"
            | "pyc" | "pyo" | "class" | "o" | "so" | "dll" | "dylib"
            | "exe" | "msi" | "dmg" => SkillFileType::Asset,

            // 二进制可识别格式
            "bin" | "dat" | "db" | "sqlite" | "sqlite3"
            | "wasm" | "jar" | "war" | "ear" => SkillFileType::Binary,

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
    /// 声明允许使用的工具列表
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// 兼容性信息（目标平台、版本等）
    #[serde(default)]
    pub compatibility: HashMap<String, String>,
    /// 其他元数据字段
    #[serde(default)]
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
    /// Skill 根目录（SingleFile 模式下为文件所在目录）
    pub skill_dir: PathBuf,
    /// skill.md 文件路径（SingleFile 模式下即为扫描目标）
    pub skill_md_path: Option<PathBuf>,
    /// 解析后的清单元数据
    pub manifest: SkillManifest,
    /// skill.md 正文（去除 front-matter 后的指令体）
    pub instruction_body: String,
    /// 所有文件列表
    pub files: Vec<SkillFile>,
    /// 引用的文件（在 skill.md 中被提及的文件）
    pub referenced_files: Vec<PathBuf>,
    /// 脚本文件子集
    pub script_files: Vec<SkillFile>,
    /// 资产文件子集
    pub asset_files: Vec<SkillFile>,
    /// 扫描策略
    pub scan_policy: ScanPolicy,
}

impl SkillContext {
    /// 创建一个新的 SkillContext
    pub fn new(scan_mode: ScanMode, skill_dir: PathBuf, scan_policy: ScanPolicy) -> Self {
        Self {
            scan_mode,
            skill_dir,
            skill_md_path: None,
            manifest: SkillManifest::default(),
            instruction_body: String::new(),
            files: Vec::new(),
            referenced_files: Vec::new(),
            script_files: Vec::new(),
            asset_files: Vec::new(),
            scan_policy,
        }
    }

    /// 获取 Skill 名称（优先使用 manifest 中的名称，否则从目录名推断）
    pub fn skill_name(&self) -> String {
        if !self.manifest.name.is_empty() {
            return self.manifest.name.clone();
        }
        self.skill_dir
            .file_name()
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
        self.script_files
            .iter()
            .map(|f| f.absolute_path.as_path())
            .collect()
    }

    /// 获取所有资产文件的路径
    pub fn asset_paths(&self) -> Vec<&Path> {
        self.asset_files
            .iter()
            .map(|f| f.absolute_path.as_path())
            .collect()
    }

    /// 检查某个文件是否在引用列表中
    pub fn is_referenced(&self, path: &Path) -> bool {
        self.referenced_files.iter().any(|p| p == path)
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
        let ctx = SkillContext::new(ScanMode::Directory, PathBuf::from("/tmp/test-skill"), policy);
        assert_eq!(ctx.scan_mode, ScanMode::Directory);
        assert_eq!(ctx.skill_dir, PathBuf::from("/tmp/test-skill"));
        assert_eq!(ctx.skill_name(), "test-skill");
        assert_eq!(ctx.file_count(), 0);
        assert_eq!(ctx.total_size_bytes(), 0);
    }

    #[test]
    fn test_skill_context_name_fallback() {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(ScanMode::SingleFile, PathBuf::from("/tmp/my-skill"), policy);
        // manifest name 为空时使用目录名
        assert_eq!(ctx.skill_name(), "my-skill");
        // manifest name 优先
        ctx.manifest.name = "custom-name".to_string();
        assert_eq!(ctx.skill_name(), "custom-name");
    }

    #[test]
    fn test_skill_context_files_by_type() {
        let policy = ScanPolicy::builtin_default().clone();
        let mut ctx = SkillContext::new(ScanMode::Directory, PathBuf::from("/tmp/skill"), policy);

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
        let mut ctx = SkillContext::new(ScanMode::Directory, PathBuf::from("/tmp/skill"), policy);

        ctx.referenced_files.push(PathBuf::from("/tmp/skill/run.sh"));
        assert!(ctx.is_referenced(Path::new("/tmp/skill/run.sh")));
        assert!(!ctx.is_referenced(Path::new("/tmp/skill/unreferenced.txt")));
    }

    #[test]
    fn test_file_type_script_coverage() {
        // 验证常见脚本扩展名都覆盖
        let script_exts = vec![
            "sh", "bash", "py", "rb", "pl", "php", "js", "mjs", "ts", "mts",
            "tsx", "jsx", "lua", "r", "rs", "go", "java", "kt", "swift", "dart",
            "ex", "hs", "elm", "v", "zig",
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
            "json", "json5", "yaml", "yml", "toml", "ini", "cfg",
            "env", "xml", "gitignore", "editorconfig",
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
}
