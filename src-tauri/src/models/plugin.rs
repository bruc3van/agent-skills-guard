use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::security::{SecurityIssue, SecurityReport};

/// Claude Code Plugin 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    /// Claude Code CLI 的插件标识：`name@marketplace`
    pub claude_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    /// 实际已安装版本（来自 `claude plugin list --json`）
    pub installed_version: Option<String>,
    pub author: Option<String>,
    pub repository_url: String,
    pub repository_owner: Option<String>,
    pub marketplace_name: String,
    pub source: String,
    /// 记录该 plugin 是从哪里发现的：`repository_scan` / `claude_cli`
    pub discovery_source: Option<String>,
    pub marketplace_add_command: Option<String>,
    pub plugin_install_command: Option<String>,
    pub installed: bool,
    pub installed_at: Option<DateTime<Utc>>,
    /// Claude Code 的安装信息（来自 `claude plugin list --json`）
    pub claude_scope: Option<String>,
    pub claude_enabled: Option<bool>,
    pub claude_install_path: Option<String>,
    pub claude_last_updated: Option<DateTime<Utc>>,
    pub security_score: Option<i32>,
    pub security_issues: Option<Vec<SecurityIssue>>,
    pub security_level: Option<String>,
    pub security_report: Option<SecurityReport>,
    pub scanned_at: Option<DateTime<Utc>>,
    pub staging_path: Option<String>,
    pub install_log: Option<String>,
    pub install_status: Option<String>,
}

impl Plugin {
    pub fn new(
        name: String,
        repository_url: String,
        marketplace_name: String,
        source: String,
    ) -> Self {
        let repository_owner = Self::parse_repository_owner(&repository_url);
        let id = format!("{}::{}::{}", repository_url, marketplace_name, name);
        let claude_id = Some(format!("{}@{}", name, marketplace_name));

        Self {
            id,
            claude_id,
            name,
            description: None,
            version: None,
            installed_version: None,
            author: None,
            repository_url,
            repository_owner: Some(repository_owner),
            marketplace_name,
            source,
            discovery_source: Some("repository_scan".to_string()),
            marketplace_add_command: None,
            plugin_install_command: None,
            installed: false,
            installed_at: None,
            claude_scope: None,
            claude_enabled: None,
            claude_install_path: None,
            claude_last_updated: None,
            security_score: None,
            security_issues: None,
            security_level: None,
            security_report: None,
            scanned_at: None,
            staging_path: None,
            install_log: None,
            install_status: None,
        }
    }

    pub fn plugin_spec(&self) -> String {
        format!("{}@{}", self.name, self.marketplace_name)
    }

    fn parse_repository_owner(repository_url: &str) -> String {
        if repository_url == "local" {
            return "local".to_string();
        }

        if let Some(start) = repository_url.find("github.com/") {
            let after_github = &repository_url[start + 11..];
            if let Some(slash_pos) = after_github.find('/') {
                return after_github[..slash_pos].to_string();
            }
        }

        "unknown".to_string()
    }
}
