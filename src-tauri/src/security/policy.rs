//! 扫描策略（ScanPolicy）
//!
//! 控制扫描行为的配置：规则启用/禁用、严重度覆盖、文件限制、
//! 文档降级、已知安装器域名等。
//!
//! 策略文件通过 `include_str!` 编译时嵌入二进制，启动时解析。
//! 不依赖运行时文件系统路径。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 扫描策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanPolicy {
    /// 策略名称
    #[serde(default = "default_policy_name")]
    pub policy_name: String,
    /// 策略版本
    #[serde(default = "default_policy_version")]
    pub policy_version: String,
    /// 预设基础
    #[serde(default)]
    pub preset_base: Option<String>,

    /// 禁用的规则 ID 列表
    #[serde(default)]
    pub disabled_rules: HashSet<String>,
    /// 严重度覆盖
    #[serde(default)]
    pub severity_overrides: Vec<SeverityOverride>,
    /// 硬触发覆盖（将规则强制设为 hard_trigger 或取消 hard_trigger）
    #[serde(default)]
    pub hard_trigger_overrides: Vec<HardTriggerOverride>,

    /// 计入安全分的 FindingKind
    #[serde(default = "default_score_kinds")]
    pub score_kinds: HashSet<String>,
    /// 是否启用严格结构校验
    #[serde(default)]
    pub strict_structure_enabled: bool,

    /// 文件限制
    #[serde(default)]
    pub file_limits: FileLimitsPolicy,
    /// 文件分类
    #[serde(default)]
    pub file_classification: FileClassificationPolicy,
    /// 规则作用域
    #[serde(default)]
    pub rule_scoping: RuleScopingPolicy,
    /// Pipeline 分析
    #[serde(default)]
    pub pipeline: PipelinePolicy,
    /// 凭据检测
    #[serde(default)]
    pub credentials: CredentialPolicy,
    /// Finding 输出
    #[serde(default)]
    pub finding_output: FindingOutputPolicy,
    /// 结构校验
    #[serde(default)]
    pub strict_structure: StrictStructurePolicy,
    /// 触发/描述质量
    #[serde(default)]
    pub trigger: TriggerPolicy,
}

/// 严重度覆盖条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityOverride {
    pub rule_id: String,
    pub severity: String, // Critical / High / Medium / Low / Info
    #[serde(default)]
    pub reason: String,
}

/// 硬触发覆盖条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardTriggerOverride {
    pub rule_id: String,
    pub hard_trigger: bool,
    #[serde(default)]
    pub reason: String,
}

/// 文件限制策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLimitsPolicy {
    /// 最大文件数（超过触发 EXCESSIVE_FILE_COUNT）
    #[serde(default = "default_max_files")]
    pub max_files: usize,
    /// 最大扫描深度
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// 单文件最大扫描字节数
    #[serde(default = "default_max_scan_file_size_bytes")]
    pub max_scan_file_size_bytes: u64,
}

impl Default for FileLimitsPolicy {
    fn default() -> Self {
        Self {
            max_files: default_max_files(),
            max_depth: default_max_depth(),
            max_scan_file_size_bytes: default_max_scan_file_size_bytes(),
        }
    }
}

/// 文件分类策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileClassificationPolicy {
    /// 惰性扩展名（静态图片、字体等，扫描时跳过）
    #[serde(default = "default_inert_extensions")]
    pub inert_extensions: HashSet<String>,
}

impl Default for FileClassificationPolicy {
    fn default() -> Self {
        Self {
            inert_extensions: default_inert_extensions(),
        }
    }
}

/// 规则作用域策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleScopingPolicy {
    /// 文档路径标识（路径中包含这些段的文件视为文档）
    #[serde(default = "default_doc_path_indicators")]
    pub doc_path_indicators: HashSet<String>,
    /// 文档中跳过的规则 ID
    #[serde(default = "default_skip_in_docs")]
    pub skip_in_docs: HashSet<String>,
}

impl Default for RuleScopingPolicy {
    fn default() -> Self {
        Self {
            doc_path_indicators: default_doc_path_indicators(),
            skip_in_docs: default_skip_in_docs(),
        }
    }
}

/// Pipeline 分析策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinePolicy {
    /// 已知安装器域名（降级处理）
    #[serde(default = "default_known_installer_domains")]
    pub known_installer_domains: HashSet<String>,
}

impl Default for PipelinePolicy {
    fn default() -> Self {
        Self {
            known_installer_domains: default_known_installer_domains(),
        }
    }
}

/// 凭据检测策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialPolicy {
    /// 已知测试凭据值（不作为真实泄露报告）
    #[serde(default = "default_known_test_values")]
    pub known_test_values: HashSet<String>,
}

impl Default for CredentialPolicy {
    fn default() -> Self {
        Self {
            known_test_values: default_known_test_values(),
        }
    }
}

/// Finding 输出策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingOutputPolicy {
    /// 是否去重
    #[serde(default = "default_true")]
    pub dedupe: bool,
}

impl Default for FindingOutputPolicy {
    fn default() -> Self {
        Self {
            dedupe: default_true(),
        }
    }
}

/// 结构校验策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrictStructurePolicy {
    /// 允许的文件扩展名
    #[serde(default = "default_allowed_extensions")]
    pub allowed_extensions: HashSet<String>,
    /// 允许的子目录名
    #[serde(default = "default_allowed_subdirs")]
    pub allowed_subdirs: HashSet<String>,
}

impl Default for StrictStructurePolicy {
    fn default() -> Self {
        Self {
            allowed_extensions: default_allowed_extensions(),
            allowed_subdirs: default_allowed_subdirs(),
        }
    }
}

/// 触发/描述质量策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPolicy {
    /// 最小描述长度（低于此触发 TRIGGER_DESCRIPTION_TOO_SHORT）
    #[serde(default = "default_min_description_length")]
    pub min_description_length: usize,
    /// 关键词诱导阈值（逗号分隔关键词超过此数量触发 TRIGGER_KEYWORD_BAITING）
    #[serde(default = "default_keyword_baiting_threshold")]
    pub keyword_baiting_threshold: usize,
}

impl Default for TriggerPolicy {
    fn default() -> Self {
        Self {
            min_description_length: default_min_description_length(),
            keyword_baiting_threshold: default_keyword_baiting_threshold(),
        }
    }
}

// ── 默认值函数 ──

fn default_policy_name() -> String {
    "default".to_string()
}
fn default_policy_version() -> String {
    "1.0".to_string()
}
fn default_max_files() -> usize {
    2000
}
fn default_max_depth() -> usize {
    20
}
fn default_max_scan_file_size_bytes() -> u64 {
    2 * 1024 * 1024 // 2 MiB
}
fn default_true() -> bool {
    true
}
fn default_min_description_length() -> usize {
    10
}
fn default_keyword_baiting_threshold() -> usize {
    8
}

fn default_inert_extensions() -> HashSet<String> {
    [
        ".ttf", ".otf", ".woff", ".woff2", ".eot", ".png", ".jpg", ".jpeg", ".gif", ".webp",
        ".avif", ".bmp", ".ico", ".icns", ".tif", ".tiff", ".heic", ".heif", ".pyc", ".pyo", ".db",
        ".sqlite", ".sqlite3",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_doc_path_indicators() -> HashSet<String> {
    [
        "doc",
        "docs",
        "references",
        "examples",
        "tutorials",
        "guides",
        "test",
        "tests",
        "fixtures",
        "samples",
        "demo",
        "skills",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_skip_in_docs() -> HashSet<String> {
    ["CURL_POST", "PY_EVAL", "TOOL_ABUSE_SYSTEM_PACKAGE_INSTALL"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_known_installer_domains() -> HashSet<String> {
    [
        "sh.rustup.rs",
        "astral.sh",
        "bun.sh",
        "deno.land",
        "get.pnpm.io",
        "nodejs.org",
        "npmjs.com",
        "pip.pypa.io",
        "brew.sh",
        "curl.se",
        "git-scm.com",
        "golang.org",
        "go.dev",
        "rustup.rs",
        "install.python-poetry.org",
        "rye.astral.sh",
        "mise.jdx.dev",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_known_test_values() -> HashSet<String> {
    [
        "sk_test_",
        "pk_test_",
        "tok_test_",
        "your-api-key-here",
        "example-token",
        "changeme",
        "password123",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_allowed_extensions() -> HashSet<String> {
    [
        ".md",
        ".py",
        ".sh",
        ".json",
        ".yaml",
        ".yml",
        ".txt",
        ".js",
        ".ts",
        ".html",
        ".css",
        ".svg",
        ".xml",
        ".xsd",
        ".toml",
        ".cfg",
        ".ini",
        ".env",
        ".gitignore",
        ".gitattributes",
        ".editorconfig",
        ".prettierrc",
        ".eslintrc",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_allowed_subdirs() -> HashSet<String> {
    [
        "scripts",
        "references",
        "assets",
        "templates",
        "data",
        "config",
        "src",
        "lib",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn default_score_kinds() -> HashSet<String> {
    ["Security", "Auditability"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

// ── 默认策略实例（编译时嵌入） ──

/// 内置默认策略 YAML
const DEFAULT_POLICY_YAML: &str = include_str!("../../resources/security/policies/default.yaml");

/// 默认策略（启动时解析一次）
static DEFAULT_POLICY: std::sync::LazyLock<ScanPolicy> = std::sync::LazyLock::new(|| {
    serde_yaml::from_str(DEFAULT_POLICY_YAML)
        .expect("Failed to parse embedded default policy YAML")
});

impl ScanPolicy {
    /// 从 YAML 字符串解析策略
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// 获取内置默认策略
    pub fn builtin_default() -> &'static ScanPolicy {
        &DEFAULT_POLICY
    }

    /// 检查规则是否被禁用
    pub fn is_rule_disabled(&self, rule_id: &str) -> bool {
        self.disabled_rules.contains(rule_id)
    }

    /// 获取规则的严重度覆盖（如果有）
    pub fn get_severity_override(&self, rule_id: &str) -> Option<&str> {
        self.severity_overrides
            .iter()
            .find(|o| o.rule_id == rule_id)
            .map(|o| o.severity.as_str())
    }

    /// 获取规则的硬触发覆盖（如果有）
    pub fn get_hard_trigger_override(&self, rule_id: &str) -> Option<bool> {
        self.hard_trigger_overrides
            .iter()
            .find(|o| o.rule_id == rule_id)
            .map(|o| o.hard_trigger)
    }

    /// 检查路径是否为文档路径（使用目录段匹配，避免子串误匹配）
    ///
    /// 匹配规则：
    /// 1. 路径段完全等于 indicator（如 `docs/file.md`）
    /// 2. 路径段以 `indicator-` 开头（如 `docs-internal/file.md`）
    pub fn is_doc_path(&self, path: &str) -> bool {
        let lower = path.replace('\\', "/").to_lowercase();
        self.rule_scoping
            .doc_path_indicators
            .iter()
            .any(|indicator| {
                let ind = indicator.to_lowercase();
                // 路径以 indicator/ 开头（如 "docs/file.md"）
                lower.starts_with(&format!("{}/", ind))
                // 路径中包含 /indicator/（如 "sub/docs/file.md"）
                || lower.contains(&format!("/{}/", ind))
                // 路径以 /indicator 结尾
                || lower.ends_with(&format!("/{}", ind))
                // 路径段以 indicator- 开头（如 "docs-internal/file.md"、"test-fixtures/data.sh"）
                || lower.split('/').any(|seg| seg.starts_with(&format!("{}-", ind)))
            })
    }

    /// 检查域名是否为已知安装器
    pub fn is_known_installer_domain(&self, domain: &str) -> bool {
        self.pipeline
            .known_installer_domains
            .iter()
            .any(|d| domain.contains(d.as_str()))
    }

    /// 计算策略指纹（用于报告追溯）
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let serialized = serde_json::to_string(self).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }
}

