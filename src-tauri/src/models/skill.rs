use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Skill 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub repository_url: String,
    pub repository_owner: Option<String>, // 仓库所有者，如 "anthropics" 或 "local"
    pub file_path: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub installed: bool,
    pub installed_at: Option<DateTime<Utc>>,
    pub local_path: Option<String>,       // 向后兼容,保留单个路径字段
    pub local_paths: Option<Vec<String>>, // 新增:支持多个安装路径
    pub checksum: Option<String>,
    pub security_score: Option<i32>,
    pub security_issues: Option<Vec<String>>,
    pub security_level: Option<String>, // 安全等级：Safe/Low/Medium/High/Critical
    pub scanned_at: Option<DateTime<Utc>>, // 扫描时间戳
    pub installed_commit_sha: Option<String>, // 安装时对应的仓库 commit SHA
}

impl Skill {
    pub fn new(name: String, repository_url: String, file_path: String) -> Self {
        // 自动解析 repository_owner
        let repository_owner = Self::parse_repository_owner(&repository_url);

        Self {
            id: format!("{}::{}", repository_url, file_path),
            name,
            description: None,
            repository_url,
            repository_owner: Some(repository_owner),
            file_path,
            version: None,
            author: None,
            installed: false,
            installed_at: None,
            local_path: None,
            local_paths: None,
            checksum: None,
            security_score: None,
            security_issues: None,
            security_level: None,
            scanned_at: None,
            installed_commit_sha: None,
        }
    }

    /// 从 repository_url 解析仓库所有者
    pub fn parse_repository_owner(repository_url: &str) -> String {
        if repository_url == "local" {
            return "local".to_string();
        }

        // 解析 GitHub URL: https://github.com/anthropics/skills
        if let Some(start) = repository_url.find("github.com/") {
            let after_github = &repository_url[start + 11..];
            if let Some(slash_pos) = after_github.find('/') {
                return after_github[..slash_pos].to_string();
            }
        }

        "unknown".to_string()
    }
}

/// Skill 安装状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillStatus {
    NotInstalled,
    Installing,
    Installed,
    Failed,
    UpdateAvailable,
}

/// Skill 安装记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallation {
    pub skill_id: String,
    pub installed_at: DateTime<Utc>,
    pub version: String,
    pub local_path: String,
    pub checksum: String,
}
