use serde::{Deserialize, Serialize};

/// 安全检查结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub skill_id: String,
    pub score: i32,
    pub level: SecurityLevel,
    pub issues: Vec<SecurityIssue>,
    pub recommendations: Vec<String>,
    pub blocked: bool,                    // 是否被硬触发规则阻止安装
    pub hard_trigger_issues: Vec<String>, // 触发的硬阻止规则列表
    pub scanned_files: Vec<String>,       // 已扫描的文件列表
    pub partial_scan: bool,               // 是否存在未完整扫描
    pub skipped_files: Vec<String>,       // 跳过扫描的文件列表
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SecurityReportMetadata>,
}

/// 报告级元数据（策略指纹等）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityReportMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_version: Option<String>,
}

/// 安全等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityLevel {
    Safe,     // 90-100
    Low,      // 70-89
    Medium,   // 50-69
    High,     // 30-49
    Critical, // 0-29
}

impl SecurityLevel {
    pub fn from_score(score: i32) -> Self {
        match score {
            90..=100 => SecurityLevel::Safe,
            70..=89 => SecurityLevel::Low,
            50..=69 => SecurityLevel::Medium,
            30..=49 => SecurityLevel::High,
            _ => SecurityLevel::Critical,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SecurityLevel::Safe => "Safe",
            SecurityLevel::Low => "Low",
            SecurityLevel::Medium => "Medium",
            SecurityLevel::High => "High",
            SecurityLevel::Critical => "Critical",
        }
    }
}

impl std::str::FromStr for SecurityLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Safe" => Ok(SecurityLevel::Safe),
            "Low" => Ok(SecurityLevel::Low),
            "Medium" => Ok(SecurityLevel::Medium),
            "High" => Ok(SecurityLevel::High),
            "Critical" => Ok(SecurityLevel::Critical),
            _ => Err(()),
        }
    }
}

/// 安全问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub description: String,
    pub line_number: Option<usize>,
    pub code_snippet: Option<String>,
    pub file_path: Option<String>, // 记录哪个文件有风险
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwe_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threat_category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_path_other_rule_ids: Option<Vec<String>>,
}

/// 问题严重程度（5 级统一模型）
///
/// 向后兼容旧的 4 级枚举（`Info`/`Warning`/`Error`/`Critical`）：
/// 旧 `Critical` → 新 `Critical`
/// 旧 `Error` → 新 `High`
/// 旧 `Warning` → 新 `Medium`
/// 旧 `Info` → 新 `Low`（旧数据中 Info 是最低级别）
///
/// 通过自定义反序列化器实现零迁移成本，无需数据迁移脚本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IssueSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl IssueSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueSeverity::Critical => "Critical",
            IssueSeverity::High => "High",
            IssueSeverity::Medium => "Medium",
            IssueSeverity::Low => "Low",
            IssueSeverity::Info => "Info",
        }
    }
}

impl<'de> serde::Deserialize<'de> for IssueSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "Critical" => Ok(IssueSeverity::Critical),
            "High" => Ok(IssueSeverity::High),
            "Medium" => Ok(IssueSeverity::Medium),
            "Low" => Ok(IssueSeverity::Low),
            "Info" => Ok(IssueSeverity::Info),
            // 旧 4 级枚举向后兼容映射
            "Error" => Ok(IssueSeverity::High),
            "Warning" => Ok(IssueSeverity::Medium),
            _ => Err(D::Error::unknown_variant(
                &s,
                &[
                    "Critical", "High", "Medium", "Low", "Info", "Error", "Warning",
                ],
            )),
        }
    }
}

/// 问题分类（对外 UI 模型，保持向后兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueCategory {
    FileSystem,        // 文件系统操作
    Network,           // 网络请求
    ProcessExecution,  // 进程执行
    DataExfiltration,  // 数据泄露风险
    DangerousFunction, // 危险函数调用
    ObfuscatedCode,    // 代码混淆
    Other,
}

/// 威胁分类（内部统一分类体系，覆盖 Cisco 语义和现有分类）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatCategory {
    Destructive,         // 破坏性操作
    RemoteExec,          // 远程下载执行
    CmdInjection,        // 命令注入
    Network,             // 网络外传
    PrivilegeEscalation, // 权限提升
    Secrets,             // 凭据/密钥泄露
    Persistence,         // 持久化
    SensitiveFileAccess, // 敏感文件访问
    PromptInjection,     // Prompt 注入/指令覆盖
    SocialEngineering,   // 社会工程/误导描述
    PolicyViolation,     // 策略违规（allowed-tools 等）
    Obfuscation,         // 混淆/伪装/隐写
}

impl ThreatCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThreatCategory::Destructive => "Destructive",
            ThreatCategory::RemoteExec => "RemoteExec",
            ThreatCategory::CmdInjection => "CmdInjection",
            ThreatCategory::Network => "Network",
            ThreatCategory::PrivilegeEscalation => "PrivilegeEscalation",
            ThreatCategory::Secrets => "Secrets",
            ThreatCategory::Persistence => "Persistence",
            ThreatCategory::SensitiveFileAccess => "SensitiveFileAccess",
            ThreatCategory::PromptInjection => "PromptInjection",
            ThreatCategory::SocialEngineering => "SocialEngineering",
            ThreatCategory::PolicyViolation => "PolicyViolation",
            ThreatCategory::Obfuscation => "Obfuscation",
        }
    }

    /// 映射到对外 IssueCategory（保持 UI 兼容）
    pub fn to_issue_category(&self) -> IssueCategory {
        match self {
            ThreatCategory::Destructive => IssueCategory::FileSystem,
            ThreatCategory::RemoteExec => IssueCategory::ProcessExecution,
            ThreatCategory::CmdInjection => IssueCategory::DangerousFunction,
            ThreatCategory::Network => IssueCategory::Network,
            ThreatCategory::PrivilegeEscalation => IssueCategory::Other,
            ThreatCategory::Secrets => IssueCategory::DataExfiltration,
            ThreatCategory::Persistence => IssueCategory::Other,
            ThreatCategory::SensitiveFileAccess => IssueCategory::FileSystem,
            ThreatCategory::PromptInjection => IssueCategory::Other,
            ThreatCategory::SocialEngineering => IssueCategory::Other,
            ThreatCategory::PolicyViolation => IssueCategory::Other,
            ThreatCategory::Obfuscation => IssueCategory::ObfuscatedCode,
        }
    }
}

/// 内部 Finding（多 analyzer 产出的统一安全发现）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// 稳定 ID：rule_id + file + line + snippet_hash
    pub id: String,
    /// 规则 ID
    pub rule_id: String,
    /// 威胁分类
    pub category: ThreatCategory,
    /// 严重程度
    pub severity: IssueSeverity,
    /// 标题
    pub title: String,
    /// 详细描述
    pub description: String,
    /// 文件路径
    pub file_path: Option<String>,
    /// 行号
    pub line_number: Option<usize>,
    /// 代码片段（secret 类 finding 需脱敏）
    pub snippet: Option<String>,
    /// 修复建议
    pub remediation: Option<String>,
    /// 产出该 finding 的分析器名称
    pub analyzer: String,
    /// 扩展元数据
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<FindingMetadata>,
}

/// Finding 扩展元数据
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingMetadata {
    /// 同路径下其他规则 ID（规则共现）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub same_path_other_rule_ids: Vec<String>,
    /// CWE 编号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwe_id: Option<String>,
    /// 置信度
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// 规则来源
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_source: Option<String>,
    /// 原始规则权重
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<i32>,
    /// 原始 hard_trigger 标记
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard_trigger: Option<bool>,
}

/// Skill 扫描结果（用于前端展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillScanResult {
    pub skill_id: String,
    pub skill_name: String,
    pub score: i32,
    pub level: String,
    pub scanned_at: String,
    pub report: SecurityReport,
}
