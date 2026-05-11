use crate::models::{LocalCliTool, PackageManager};
use crate::services::Database;
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;

const CACHE_TTL_SECS: i64 = 3600;

pub(crate) fn is_cache_fresh(last_checked: Option<&str>) -> bool {
    let Some(ts) = last_checked else {
        return false;
    };
    let Ok(t) = ts.parse::<chrono::DateTime<Utc>>() else {
        return false;
    };
    Utc::now().signed_duration_since(t).num_seconds() < CACHE_TTL_SECS
}

pub(crate) fn is_outdated(current: Option<&str>, latest: Option<&str>) -> bool {
    match (current, latest) {
        (Some(c), Some(l)) => c != l,
        _ => false,
    }
}

async fn http_get_json(url: &str) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("agent-skills-guard/1.0")
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("{} → HTTP {}", url, resp.status());
    }
    Ok(resp.json().await?)
}

pub async fn fetch_npm_latest(name: &str) -> Result<String> {
    let body =
        http_get_json(&format!("https://registry.npmjs.org/{}/latest", name)).await?;
    body["version"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("npm registry 响应无 version 字段"))
}

pub async fn fetch_pypi_latest(name: &str) -> Result<String> {
    let body =
        http_get_json(&format!("https://pypi.org/pypi/{}/json", name)).await?;
    body["info"]["version"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("PyPI 响应无 version 字段"))
}

pub struct LocalCliUpdater {
    db: Arc<Database>,
}

impl LocalCliUpdater {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn check_updates(&self, tools: &mut Vec<LocalCliTool>) -> Result<()> {
        for tool in tools.iter_mut() {
            if is_cache_fresh(tool.last_checked.as_deref()) {
                continue;
            }

            let latest_result = match tool.manager {
                PackageManager::Npm => fetch_npm_latest(&tool.id).await,
                PackageManager::Pip => fetch_pypi_latest(&tool.id).await,
                _ => continue,
            };

            match latest_result {
                Ok(latest) => {
                    tool.update_available =
                        is_outdated(tool.current_version.as_deref(), Some(&latest));
                    tool.latest_version = Some(latest.clone());
                    tool.last_checked = Some(Utc::now().to_rfc3339());
                    let _ = self.db.upsert_local_cli_tool(
                        &tool.id,
                        &tool.detected_path,
                        tool.manager.as_str(),
                        tool.current_version.as_deref(),
                        Some(&latest),
                        tool.update_available,
                        tool.last_checked.as_deref(),
                    );
                }
                Err(e) => log::warn!("检查 {} 更新失败: {}", tool.id, e),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_fresh_within_one_hour() {
        let ts = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        assert!(is_cache_fresh(Some(&ts)));
    }

    #[test]
    fn cache_stale_after_one_hour() {
        let ts = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        assert!(!is_cache_fresh(Some(&ts)));
    }

    #[test]
    fn version_is_outdated_when_latest_differs() {
        assert!(is_outdated(Some("0.3.1"), Some("0.4.0")));
        assert!(!is_outdated(Some("0.4.0"), Some("0.4.0")));
        assert!(!is_outdated(None, Some("0.4.0")));
    }
}
