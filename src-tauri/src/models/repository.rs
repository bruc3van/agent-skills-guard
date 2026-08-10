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

    /// 将用户常见的 GitHub 仓库写法规范化为 HTTPS URL，并立即验证仓库路径。
    pub fn normalize_github_url(url: &str) -> Result<String> {
        let trimmed = url.trim();
        let lower = trimmed.to_ascii_lowercase();
        let prefixes = [
            "https://www.github.com/",
            "http://www.github.com/",
            "https://github.com/",
            "http://github.com/",
            "www.github.com/",
            "github.com/",
        ];
        let path = prefixes
            .iter()
            .find_map(|prefix| lower.starts_with(prefix).then(|| &trimmed[prefix.len()..]))
            .ok_or_else(|| {
                anyhow!(
                    "REPOSITORY_URL_UNSUPPORTED: 仅支持 GitHub 仓库，请检查地址或删除旧记录后重新添加: {}",
                    url
                )
            })?;
        let normalized = format!("https://github.com/{path}");
        let (owner, repo) = Self::from_github_url(&normalized)?;
        Ok(format!("https://github.com/{owner}/{repo}"))
    }

    /// 从 GitHub URL 提取仓库信息
    /// 支持格式:
    ///   https://github.com/owner/repo
    ///   https://github.com/owner/repo.git
    ///   https://github.com/owner/repo/
    ///   https://github.com/owner/repo/tree/branch
    ///   https://github.com/owner/repo/blob/branch/file
    pub fn from_github_url(url: &str) -> Result<(String, String)> {
        let original = url;
        let path = url
            .trim()
            .strip_prefix("https://github.com/")
            .ok_or_else(|| {
                anyhow!(
                    "REPOSITORY_URL_UNSUPPORTED: 仅支持 GitHub 仓库，请删除旧记录后重新添加: {}",
                    original
                )
            })?;
        let mut parts = path.trim_end_matches('/').split('/');
        let owner = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default().trim_end_matches(".git");
        let suffix = parts.next();

        if !valid_github_owner(owner) || !valid_github_repo(repo) {
            return Err(anyhow!(
                "Invalid GitHub owner or repository name: {}",
                original
            ));
        }

        // 只接受仓库根 URL，或 GitHub 标准的 tree/blob 页面 URL。这样 owner/repo
        // 不会从任意域名或形似路径的字符串中被“倒数两段”误提取出来。
        if let Some(kind) = suffix {
            if !matches!(kind, "tree" | "blob") || parts.next().is_none() {
                return Err(anyhow!("Invalid GitHub repository URL path: {}", original));
            }
        }

        Ok((owner.to_string(), repo.to_string()))
    }
}

fn valid_github_owner(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 39
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_github_repo(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_github_url_handles_basic_url() {
        let (owner, repo) =
            Repository::from_github_url("https://github.com/anthropics/skills").unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
    }

    #[test]
    fn from_github_url_strips_tree_branch() {
        let (owner, repo) =
            Repository::from_github_url("https://github.com/anthropics/skills/tree/main").unwrap();
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
    }

    #[test]
    fn from_github_url_strips_blob_path() {
        let (owner, repo) =
            Repository::from_github_url("https://github.com/owner/repo/blob/main/README.md")
                .unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn from_github_url_strips_git_suffix() {
        let (owner, repo) =
            Repository::from_github_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn from_github_url_strips_trailing_slash() {
        let (owner, repo) = Repository::from_github_url("https://github.com/owner/repo/").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn from_github_url_rejects_other_hosts_and_path_segments() {
        assert!(Repository::from_github_url("https://example.com/owner/repo").is_err());
        assert!(Repository::from_github_url("https://github.com/../repo").is_err());
        assert!(Repository::from_github_url("https://github.com/owner/repo%2Fother").is_err());
        assert!(Repository::from_github_url("https://github.com/owner/repo/issues").is_err());
    }

    #[test]
    fn normalize_github_url_accepts_common_user_input() {
        assert_eq!(
            Repository::normalize_github_url("github.com/owner/repo").unwrap(),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            Repository::normalize_github_url("http://www.github.com/owner/repo.git").unwrap(),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            Repository::normalize_github_url(
                "HTTPS://GITHUB.COM/owner/repo/tree/main/skills/example/"
            )
            .unwrap(),
            "https://github.com/owner/repo"
        );
        assert!(
            Repository::normalize_github_url("https://example.com/owner/repo")
                .unwrap_err()
                .to_string()
                .contains("REPOSITORY_URL_UNSUPPORTED")
        );
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
