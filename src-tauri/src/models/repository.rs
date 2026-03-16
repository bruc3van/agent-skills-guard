use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// GitHub 仓库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub id: String,
    pub url: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub scan_subdirs: bool,
    pub added_at: DateTime<Utc>,
    pub last_scanned: Option<DateTime<Utc>>,
    // 新增：缓存相关字段
    pub cache_path: Option<String>,
    pub cached_at: Option<DateTime<Utc>>,
    pub cached_commit_sha: Option<String>,
}

impl Repository {
    pub fn new(url: String, name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            url,
            name,
            description: None,
            enabled: true,
            scan_subdirs: true,
            added_at: Utc::now(),
            last_scanned: None,
            cache_path: None,
            cached_at: None,
            cached_commit_sha: None,
        }
    }

    /// 从 GitHub URL 提取仓库信息
    /// 支持格式: https://github.com/owner/repo
    pub fn from_github_url(url: &str) -> Result<(String, String)> {
        let url = url.trim_end_matches('/');
        let parts: Vec<&str> = url.split('/').collect();

        if parts.len() < 2 {
            return Err(anyhow!("Invalid GitHub URL"));
        }

        let owner = parts[parts.len() - 2].to_string();
        let repo = parts[parts.len() - 1].to_string();

        Ok((owner, repo))
    }
}

/// GitHub API 响应 - 目录内容
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubContent {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub download_url: Option<String>,
    pub sha: String,
    pub size: u64,
}
