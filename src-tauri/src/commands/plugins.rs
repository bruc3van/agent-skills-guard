use crate::commands::featured_marketplaces;
use crate::commands::AppState;
use crate::i18n::validate_locale;
use crate::models::{Plugin, SecurityReport};
use crate::security::{ScanOptions, SecurityScanner};
use crate::services::plugin_manager::{
    ClaudeMarketplace, MarketplaceRemoveResult, MarketplaceUpdateResult, PluginInstallResult,
    PluginUninstallResult, PluginUpdateResult, SkillPluginUpgradeCandidate,
};
use chrono::Utc;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, Clone)]
struct ScanProgressEvent {
    scan_id: String,
    kind: String,
    item_id: String,
    file_path: String,
}

/// 获取所有 plugins
#[tauri::command]
pub async fn get_plugins(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    locale: Option<String>,
) -> Result<Vec<Plugin>, String> {
    let locale = validate_locale(locale.as_deref().unwrap_or("en"));
    let featured_config = featured_marketplaces::get_featured_marketplaces(app)
        .await
        .ok();

    // 通过 Claude CLI 同步本地安装状态（包含非本程序安装的 plugins/marketplaces）
    // 同步失败不阻塞 UI：回退到 DB 缓存
    if let Ok(manager) = state.plugin_manager.try_lock() {
        if let Some(config) = &featured_config {
            if let Err(e) = manager
                .sync_featured_marketplaces(config, &locale, None, true)
                .await
            {
                log::warn!("同步精选插件清单失败: {}", e);
            }
        }
        if let Err(e) = manager.sync_claude_installed_state(None).await {
            log::warn!("同步 Claude plugins 状态失败: {}", e);
        }
    } else {
        let manager = state.plugin_manager.lock().await;
        if let Some(config) = &featured_config {
            if let Err(e) = manager
                .sync_featured_marketplaces(config, &locale, None, true)
                .await
            {
                log::warn!("同步精选插件清单失败: {}", e);
            }
        }
        if let Err(e) = manager.sync_claude_installed_state(None).await {
            log::warn!("同步 Claude plugins 状态失败: {}", e);
        }
    }

    state.db.get_plugins().map_err(|e| e.to_string())
}

/// 获取 plugins（不触发 CLI 同步，仅读取 DB）
#[tauri::command]
pub async fn get_plugins_cached(state: State<'_, AppState>) -> Result<Vec<Plugin>, String> {
    state.db.get_plugins().map_err(|e| e.to_string())
}

/// 仅同步精选市场插件清单（不触发 Claude CLI）
#[tauri::command]
pub async fn sync_featured_marketplace_plugins(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    locale: String,
) -> Result<Vec<Plugin>, String> {
    let locale = validate_locale(&locale);
    let featured_config = featured_marketplaces::get_featured_marketplaces(app)
        .await
        .map_err(|e| e.to_string())?;

    let manager = state.plugin_manager.lock().await;
    manager
        .sync_featured_marketplaces(&featured_config, &locale, None, false)
        .await
        .map_err(|e| e.to_string())?;

    state.db.get_plugins().map_err(|e| e.to_string())
}

/// 准备安装 plugin：下载并扫描 marketplace repo
#[tauri::command]
pub async fn prepare_plugin_installation(
    state: State<'_, AppState>,
    plugin_id: String,
    locale: String,
) -> Result<SecurityReport, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .prepare_plugin_installation(&plugin_id, &locale)
        .await
        .map_err(|e| e.to_string())
}

/// 确认安装 plugin：驱动 Claude Code CLI 执行安装
#[tauri::command]
pub async fn confirm_plugin_installation(
    state: State<'_, AppState>,
    plugin_id: String,
    claude_command: Option<String>,
) -> Result<PluginInstallResult, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .confirm_plugin_installation(&plugin_id, claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 取消 plugin 安装准备状态
#[tauri::command]
pub async fn cancel_plugin_installation(
    state: State<'_, AppState>,
    plugin_id: String,
) -> Result<(), String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .cancel_plugin_installation(&plugin_id)
        .map_err(|e| e.to_string())
}

/// 卸载 plugin
#[tauri::command]
pub async fn uninstall_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    claude_command: Option<String>,
) -> Result<PluginUninstallResult, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .uninstall_plugin(&plugin_id, claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 移除整个 marketplace（会自动卸载该 marketplace 的所有 plugins）
#[tauri::command]
pub async fn remove_marketplace(
    state: State<'_, AppState>,
    marketplace_name: String,
    marketplace_repo: String,
    claude_command: Option<String>,
) -> Result<MarketplaceRemoveResult, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .remove_marketplace(&marketplace_name, &marketplace_repo, claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 获取 Claude Code 已配置的 marketplaces（来自 CLI）
#[tauri::command]
pub async fn get_claude_marketplaces(
    state: State<'_, AppState>,
    claude_command: Option<String>,
) -> Result<Vec<ClaudeMarketplace>, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .get_claude_marketplaces(claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 检查已安装 plugins 的更新（来自 CLI）
/// 返回：Vec<(plugin_id, latest_version)>
#[tauri::command]
pub async fn check_plugins_updates(
    state: State<'_, AppState>,
    claude_command: Option<String>,
) -> Result<Vec<(String, String)>, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .check_plugins_updates(claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 更新单个 plugin（调用 Claude Code CLI）
#[tauri::command]
pub async fn update_plugin(
    state: State<'_, AppState>,
    plugin_id: String,
    claude_command: Option<String>,
) -> Result<PluginUpdateResult, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .update_plugin(&plugin_id, claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 检查 marketplaces 的更新（基于本地安装目录的 git HEAD 对比）
/// 返回：Vec<(marketplace_name, latest_head_short_sha)>
#[tauri::command]
pub async fn check_marketplaces_updates(
    state: State<'_, AppState>,
    claude_command: Option<String>,
) -> Result<Vec<(String, String)>, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .check_marketplaces_updates(claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 更新单个 marketplace（调用 Claude Code CLI）
#[tauri::command]
pub async fn update_marketplace(
    state: State<'_, AppState>,
    marketplace_name: String,
    claude_command: Option<String>,
) -> Result<MarketplaceUpdateResult, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .update_marketplace(&marketplace_name, claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 检测已安装 skills 中可升级为 Claude Code Plugin 的候选项
#[tauri::command]
pub async fn get_skill_plugin_upgrade_candidates(
    state: State<'_, AppState>,
    claude_command: Option<String>,
) -> Result<Vec<SkillPluginUpgradeCandidate>, String> {
    let manager = state.plugin_manager.lock().await;
    manager
        .get_skill_plugin_upgrade_candidates(claude_command)
        .await
        .map_err(|e| e.to_string())
}

/// 安全扫描所有已安装 plugins（读取 Claude CLI 提供的 installPath）
///
/// 返回：成功扫描的 plugin_id 列表（数据库 id）
#[tauri::command]
pub async fn scan_all_installed_plugins(
    state: State<'_, AppState>,
    locale: String,
    claude_command: Option<String>,
    scan_parallelism: Option<usize>,
) -> Result<Vec<String>, String> {
    let locale = validate_locale(&locale);

    // 先同步 Claude CLI 的安装状态，确保 installPath 最新
    {
        let manager = state.plugin_manager.lock().await;
        if let Err(e) = manager.sync_claude_installed_state(claude_command).await {
            log::warn!("同步 Claude plugins 状态失败（将继续扫描 DB 记录）: {}", e);
        }
    }

    let plugins = state.db.get_plugins().map_err(|e| e.to_string())?;
    let installed_plugins: Vec<Plugin> = plugins.into_iter().filter(|p| p.installed).collect();

    const DEFAULT_SCAN_PARALLELISM: usize = 3;
    const MAX_SCAN_PARALLELISM: usize = 8;
    let parallelism = scan_parallelism
        .unwrap_or(DEFAULT_SCAN_PARALLELISM)
        .clamp(1, MAX_SCAN_PARALLELISM);

    let db = state.db.clone();
    let locale_owned = locale.to_string();

    let pool = ThreadPoolBuilder::new()
        .num_threads(parallelism)
        .build()
        .map_err(|e| e.to_string())?;

    let mut scanned = pool.install(|| {
        installed_plugins
            .par_iter()
            .enumerate()
            .filter_map(|(index, plugin)| {
                let Some(install_path) = plugin.claude_install_path.clone() else {
                    return None;
                };
                let path = PathBuf::from(&install_path);
                if !path.exists() || !path.is_dir() {
                    log::warn!("Plugin directory does not exist: {:?}", path);
                    return None;
                }

                let scanner = SecurityScanner::new();
                let report = match scanner.scan_directory_with_options(
                    path.to_str().unwrap_or(""),
                    &plugin.id,
                    &locale_owned,
                    ScanOptions { skip_readme: true },
                    None,
                ) {
                    Ok(report) => report,
                    Err(e) => {
                        log::warn!("Failed to scan plugin {}: {}", plugin.name, e);
                        return None;
                    }
                };

                let mut updated = plugin.clone();
                updated.security_score = Some(report.score);
                updated.security_level = Some(report.level.as_str().to_string());
                updated.security_issues = Some(
                    report
                        .issues
                        .iter()
                        .map(|i| {
                            let file_info = i
                                .file_path
                                .as_ref()
                                .map(|f| format!("[{}] ", f))
                                .unwrap_or_default();
                            format!("{}{:?}: {}", file_info, i.severity, i.description)
                        })
                        .collect(),
                );
                updated.scanned_at = Some(Utc::now());

                if let Err(e) = db.save_plugin(&updated) {
                    log::warn!("Failed to save plugin {}: {}", updated.name, e);
                    return None;
                }

                Some((index, updated.id.clone()))
            })
            .collect::<Vec<(usize, String)>>()
    });

    scanned.sort_by_key(|(index, _)| *index);
    Ok(scanned.into_iter().map(|(_, id)| id).collect())
}

/// 安全扫描单个已安装 plugin（用于前端展示扫描进度）
#[tauri::command]
pub async fn scan_installed_plugin(
    state: State<'_, AppState>,
    app: AppHandle,
    plugin_id: String,
    locale: String,
    claude_command: Option<String>,
    scan_id: Option<String>,
    skip_sync: Option<bool>,
) -> Result<String, String> {
    let locale = validate_locale(&locale);

    // 尝试同步 installPath（不强制成功）
    if !skip_sync.unwrap_or(false) {
        let manager = state.plugin_manager.lock().await;
        if let Err(e) = manager.sync_claude_installed_state(claude_command).await {
            log::debug!("同步 Claude plugins 状态失败: {}", e);
        }
    }

    let mut plugin = state
        .db
        .get_plugins()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == plugin_id)
        .ok_or_else(|| "Plugin not found".to_string())?;

    if !plugin.installed {
        return Err("Plugin is not installed".to_string());
    }

    let Some(install_path) = plugin.claude_install_path.clone() else {
        return Err("Plugin install path is not available".to_string());
    };

    let path = PathBuf::from(&install_path);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Plugin directory does not exist: {}", install_path));
    }

    let scanner = SecurityScanner::new();
    let report = if let Some(scan_id) = scan_id.filter(|id| !id.is_empty()) {
        let app_handle = app.clone();
        let item_id = plugin.id.clone();
        let kind = "plugin".to_string();
        let mut progress_cb = |file_path: &str| {
            let payload = ScanProgressEvent {
                scan_id: scan_id.clone(),
                kind: kind.clone(),
                item_id: item_id.clone(),
                file_path: file_path.to_string(),
            };
            let _ = app_handle.emit("scan-progress", payload);
        };
        scanner
            .scan_directory_with_options(
                path.to_str().unwrap_or(""),
                &plugin.id,
                &locale,
                ScanOptions { skip_readme: true },
                Some(&mut progress_cb),
            )
            .map_err(|e| e.to_string())?
    } else {
        scanner
            .scan_directory_with_options(
                path.to_str().unwrap_or(""),
                &plugin.id,
                &locale,
                ScanOptions { skip_readme: true },
                None,
            )
            .map_err(|e| e.to_string())?
    };

    plugin.security_score = Some(report.score);
    plugin.security_level = Some(report.level.as_str().to_string());
    plugin.security_issues = Some(
        report
            .issues
            .iter()
            .map(|i| {
                let file_info = i
                    .file_path
                    .as_ref()
                    .map(|f| format!("[{}] ", f))
                    .unwrap_or_default();
                format!("{}{:?}: {}", file_info, i.severity, i.description)
            })
            .collect(),
    );
    plugin.scanned_at = Some(Utc::now());

    state
        .db
        .save_plugin(&plugin)
        .map_err(|e| format!("Failed to save plugin: {}", e))?;

    Ok(plugin.id)
}
