use crate::models::FeaturedMarketplacesConfig;
use std::path::PathBuf;
use tauri::Manager;

pub const FEATURED_MARKETPLACES_REMOTE_URL: &str =
    "https://raw.githubusercontent.com/bruc3van/agent-skills-guard/main/featured-marketplace.yaml";
const DEFAULT_FEATURED_MARKETPLACES_YAML: &str = include_str!("../../../featured-marketplace.yaml");

fn featured_marketplaces_cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;

    Ok(app_dir.join("featured-marketplace.yaml"))
}

/// 删除损坏或过期的 YAML 缓存文件（文件不存在时忽略）。
pub fn remove_cache_file(cache_path: &PathBuf) {
    match std::fs::remove_file(cache_path) {
        Ok(()) => log::info!("已删除精选配置缓存: {:?}", cache_path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("删除精选配置缓存失败: {:?}, 错误: {}", cache_path, e),
    }
}

/// 从远程 URL 下载 YAML 内容。
pub async fn download_remote_yaml(url: &str) -> Result<String, String> {
    use std::time::Duration;

    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?
        .get(url)
        .header(reqwest::header::USER_AGENT, "agent-skills-guard")
        .send()
        .await
        .map_err(|e| format!("Failed to download: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Failed to download: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read content: {}", e))
}

/// 将 YAML 内容原子写入缓存文件。
pub fn write_yaml_cache(cache_path: &PathBuf, yaml_content: &str) -> Result<(), String> {
    use std::io::Write;

    let cache_dir = cache_path
        .parent()
        .ok_or_else(|| "Failed to get cache directory".to_string())?;

    let mut tmp = tempfile::NamedTempFile::new_in(cache_dir)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(yaml_content.as_bytes())
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    tmp.persist(cache_path)
        .map_err(|e| format!("Failed to persist cache: {}", e))?;

    Ok(())
}

/// 获取精选插件市场列表
#[tauri::command]
pub async fn get_featured_marketplaces(
    app: tauri::AppHandle,
) -> Result<FeaturedMarketplacesConfig, String> {
    // 1) 优先读取 app_data_dir 下的缓存文件（支持在线刷新后持久化）
    let cache_path = featured_marketplaces_cache_path(&app)?;
    if let Ok(cached_yaml) = std::fs::read_to_string(&cache_path) {
        match serde_yaml::from_str::<FeaturedMarketplacesConfig>(&cached_yaml) {
            Ok(config) => return Ok(config),
            Err(e) => {
                log::warn!(
                    "精选插件市场缓存文件解析失败，将回退到内置默认配置: {:?}, 错误: {}",
                    cache_path,
                    e
                );
                remove_cache_file(&cache_path);
            }
        }
    }

    // 2) 回退到编译期内置的默认 YAML（用于首次启动/离线/打包环境）
    serde_yaml::from_str::<FeaturedMarketplacesConfig>(DEFAULT_FEATURED_MARKETPLACES_YAML)
        .map_err(|e| format!("Failed to parse default featured marketplaces: {}", e))
}

/// 刷新精选插件市场列表（从 GitHub 下载最新 YAML 并写入 app_data_dir 缓存）
#[tauri::command]
pub async fn refresh_featured_marketplaces(
    app: tauri::AppHandle,
) -> Result<FeaturedMarketplacesConfig, String> {
    let cache_path = featured_marketplaces_cache_path(&app)?;
    let yaml_content = download_remote_yaml(FEATURED_MARKETPLACES_REMOTE_URL).await?;

    let config = serde_yaml::from_str::<FeaturedMarketplacesConfig>(&yaml_content)
        .map_err(|e| {
            remove_cache_file(&cache_path);
            format!("Failed to parse downloaded featured marketplaces: {}", e)
        })?;

    write_yaml_cache(&cache_path, &yaml_content)?;
    Ok(config)
}
