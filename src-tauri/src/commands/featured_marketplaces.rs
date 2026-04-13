use crate::models::FeaturedMarketplacesConfig;
use std::path::PathBuf;
use tauri::Manager;

const FEATURED_MARKETPLACES_REMOTE_URL: &str =
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
    use std::io::Write;

    let yaml_content = reqwest::Client::new()
        .get(FEATURED_MARKETPLACES_REMOTE_URL)
        .header(reqwest::header::USER_AGENT, "agent-skills-guard")
        .send()
        .await
        .map_err(|e| format!("Failed to download featured marketplaces: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Failed to download featured marketplaces: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read featured marketplaces content: {}", e))?;

    // 先校验解析成功，再落盘
    let config: FeaturedMarketplacesConfig = serde_yaml::from_str(&yaml_content)
        .map_err(|e| format!("Failed to parse downloaded featured marketplaces: {}", e))?;

    let cache_path = featured_marketplaces_cache_path(&app)?;
    let cache_dir = cache_path
        .parent()
        .ok_or_else(|| "Failed to get featured marketplaces cache directory".to_string())?;

    let mut tmp = tempfile::NamedTempFile::new_in(cache_dir)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(yaml_content.as_bytes())
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    if cache_path.exists() {
        let _ = std::fs::remove_file(&cache_path);
    }
    tmp.persist(&cache_path)
        .map_err(|e| format!("Failed to persist featured marketplaces cache: {}", e))?;

    Ok(config)
}
