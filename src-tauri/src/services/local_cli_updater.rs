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
        (Some(c), Some(l)) => normalize_version(c) != normalize_version(l),
        _ => false,
    }
}

fn normalize_version(v: &str) -> &str {
    v.strip_prefix('v').unwrap_or(v)
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
    let body = http_get_json(&format!("https://registry.npmjs.org/{}/latest", name)).await?;
    body["version"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("npm registry 响应无 version 字段"))
}

pub async fn fetch_pypi_latest(name: &str) -> Result<String> {
    let body = http_get_json(&format!("https://pypi.org/pypi/{}/json", name)).await?;
    body["info"]["version"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("PyPI 响应无 version 字段"))
}

pub async fn fetch_brew_latest(name: &str) -> Result<String> {
    let body =
        http_get_json(&format!("https://formulae.brew.sh/api/formula/{}.json", name)).await?;
    body["versions"]["stable"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("Homebrew API 响应无 versions.stable 字段"))
}

pub async fn fetch_scoop_latest(name: &str) -> Result<String> {
    const BUCKETS: &[&str] = &["Main", "Extras", "Versions"];
    for bucket in BUCKETS {
        let url = format!(
            "https://raw.githubusercontent.com/ScoopInstaller/{bucket}/master/bucket/{name}.json"
        );
        if let Ok(body) = http_get_json(&url).await {
            if let Some(version) = body["version"].as_str() {
                return Ok(version.to_string());
            }
        }
    }
    anyhow::bail!("Scoop 未在常用 bucket 中找到 {}", name)
}

pub async fn fetch_choco_latest(name: &str) -> Result<String> {
    let url = format!(
        "https://community.chocolatey.org/api/v2/Packages()?$filter=Id%20eq%20%27{}%27&$orderby=Version%20desc&$top=1",
        name
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("agent-skills-guard/1.0")
        .build()?;
    let resp = client.get(&url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("Chocolatey API → HTTP {}", resp.status());
    }
    let xml = resp.text().await?;
    extract_choco_version(&xml)
        .ok_or_else(|| anyhow::anyhow!("Chocolatey 响应未找到版本号"))
}

fn extract_choco_version(xml: &str) -> Option<String> {
    let start = xml.find("<d:Version>")? + "<d:Version>".len();
    let end = xml[start..].find("</d:Version>")?;
    Some(xml[start..start + end].to_string())
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
                tool.update_available = is_outdated(
                    tool.current_version.as_deref(),
                    tool.latest_version.as_deref(),
                );
                let _ = self.db.upsert_local_cli_tool(
                    &tool.id,
                    &tool.detected_path,
                    tool.manager.as_str(),
                    tool.current_version.as_deref(),
                    tool.latest_version.as_deref(),
                    tool.update_available,
                    tool.last_checked.as_deref(),
                    tool.package_name.as_deref(),
                    tool.description.as_deref(),
                );
                continue;
            }

            let pkg_name = match tool.effective_package_name() {
                Some(name) => name.to_string(),
                None => continue,
            };

            let latest_result = match tool.manager {
                PackageManager::Npm | PackageManager::Pnpm => fetch_npm_latest(&pkg_name).await,
                PackageManager::Pip => fetch_pypi_latest(&pkg_name).await,
                PackageManager::Brew => fetch_brew_latest(&pkg_name).await,
                PackageManager::Scoop => fetch_scoop_latest(&pkg_name).await,
                PackageManager::Choco => fetch_choco_latest(&pkg_name).await,
                PackageManager::Unknown => continue,
            };

            match latest_result {
                Ok(latest) => {
                    let latest = latest.strip_prefix('v').unwrap_or(&latest).to_string();
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
                        tool.package_name.as_deref(),
                        tool.description.as_deref(),
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

    #[tokio::test]
    async fn fresh_cache_recomputes_update_flag_from_detected_current_version() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(dir.path().join("test.db")).unwrap());
        let updater = LocalCliUpdater::new(Arc::clone(&db));
        let mut tool =
            LocalCliTool::new("bdc", r"C:\Python314\Scripts\bdc.exe", PackageManager::Pip);
        tool.current_version = Some("0.1.2".to_string());
        tool.latest_version = Some("0.1.3".to_string());
        tool.update_available = false;
        tool.last_checked = Some(Utc::now().to_rfc3339());

        let mut tools = vec![tool];

        updater.check_updates(&mut tools).await.unwrap();

        assert!(tools[0].update_available);
    }
}
