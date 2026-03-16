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

/// 安全问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub description: String,
    pub line_number: Option<usize>,
    pub code_snippet: Option<String>,
    pub file_path: Option<String>, // 记录哪个文件有风险
}

/// 问题严重程度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// 问题分类
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
