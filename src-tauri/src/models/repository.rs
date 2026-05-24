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
    /// 支持格式:
    ///   https://github.com/owner/repo
    ///   https://github.com/owner/repo.git
    ///   https://github.com/owner/repo/
    ///   https://github.com/owner/repo/tree/branch
    ///   https://github.com/owner/repo/blob/branch/file
    pub fn from_github_url(url: &str) -> Result<(String, String)> {
        let url = url.trim_end_matches('/');

        // 去掉 /tree/... 或 /blob/... 后缀（GitHub 页面分享的 URL 格式）
        let url = if let Some(pos) = url.find("/tree/") {
            &url[..pos]
        } else if let Some(pos) = url.find("/blob/") {
            &url[..pos]
        } else {
            url
        };

        let parts: Vec<&str> = url.split('/').collect();

        if parts.len() < 2 {
            return Err(anyhow!("Invalid GitHub URL: {}", url));
        }

        let owner = parts[parts.len() - 2].to_string();
        let repo = parts[parts.len() - 1].trim_end_matches(".git").to_string();

        if owner.is_empty() || repo.is_empty() {
            return Err(anyhow!("Invalid GitHub URL (empty owner or repo): {}", url));
        }

        Ok((owner, repo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_github_url_handles_basic_url() {
        let (owner, repo) = Repository::from_github_url("https://github.com/anthropics/skills").unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
    }

    #[test]
    fn from_github_url_strips_tree_branch() {
        let (owner, repo) = Repository::from_github_url("https://github.com/anthropics/skills/tree/main").unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
    }

    #[test]
    fn from_github_url_strips_blob_path() {
        let (owner, repo) = Repository::from_github_url("https://github.com/owner/repo/blob/main/README.md").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn from_github_url_strips_git_suffix() {
        let (owner, repo) = Repository::from_github_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn from_github_url_strips_trailing_slash() {
        let (owner, repo) = Repository::from_github_url("https://github.com/owner/repo/").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
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
