use crate::commands::AppState;
use crate::models::{LocalCliTool, PackageManager};
use crate::services::claude_cli::{ClaudeCli, ClaudeCommand};
use crate::services::{discover_local_cli_tools, resolve_description_for_path, LocalCliUpdater};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

pub fn build_pty_update_args(tool: &LocalCliTool) -> Option<(String, Vec<String>)> {
    let pkg = tool.package_name.as_deref()?;
    let (bin, args) = match tool.manager {
        PackageManager::Npm => (
            "npm",
            vec!["install".to_string(), "-g".to_string(), pkg.to_string()],
        ),
        PackageManager::Pip => (
            "pip",
            vec![
                "install".to_string(),
                "--upgrade".to_string(),
                pkg.to_string(),
            ],
        ),
        PackageManager::Brew => ("brew", vec!["upgrade".to_string(), pkg.to_string()]),
        PackageManager::Scoop => ("scoop", vec!["update".to_string(), pkg.to_string()]),
        PackageManager::Choco => (
            "choco",
            vec!["upgrade".to_string(), pkg.to_string(), "-y".to_string()],
        ),
        PackageManager::Unknown => return None,
    };
    Some((bin.to_string(), args))
}

pub fn build_pty_uninstall_args(tool: &LocalCliTool) -> Option<(String, Vec<String>)> {
    let pkg = tool.package_name.as_deref()?;
    let (bin, args) = match tool.manager {
        PackageManager::Npm => (
            "npm",
            vec!["uninstall".to_string(), "-g".to_string(), pkg.to_string()],
        ),
        PackageManager::Pip => (
            "pip",
            vec!["uninstall".to_string(), "-y".to_string(), pkg.to_string()],
        ),
        PackageManager::Brew => ("brew", vec!["uninstall".to_string(), pkg.to_string()]),
        PackageManager::Scoop => ("scoop", vec!["uninstall".to_string(), pkg.to_string()]),
        PackageManager::Choco => (
            "choco",
            vec!["uninstall".to_string(), pkg.to_string(), "-y".to_string()],
        ),
        PackageManager::Unknown => return None,
    };
    Some((bin.to_string(), args))
}

#[tauri::command]
pub async fn list_local_cli_tools(state: State<'_, AppState>) -> Result<Vec<LocalCliTool>, String> {
    let mut tools = tokio::task::spawn_blocking(discover_local_cli_tools)
        .await
        .map_err(|e| e.to_string())?;

    let cached = state
        .db
        .get_all_local_cli_tools()
        .map_err(|e| e.to_string())?;
    let cache_map: std::collections::HashMap<String, _> = cached
        .into_iter()
        .map(
            |(id, path, mgr, cur, lat, upd, chk, status, log, pkg, desc)| {
                (id, (path, mgr, cur, lat, upd, chk, status, log, pkg, desc))
            },
        )
        .collect();

    for tool in tools.iter_mut() {
        if let Some((_, _, _, latest, update_avail, checked, status, log, _pkg, desc)) =
            cache_map.get(&tool.id)
        {
            tool.latest_version = latest.clone();
            tool.update_available = *update_avail;
            tool.last_checked = checked.clone();
            tool.update_status = status.clone();
            tool.update_log = log.clone();
            if tool.description.is_none() {
                tool.description = desc.clone();
            }
        }
        let _ = state.db.upsert_local_cli_tool(
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
    }

    Ok(tools)
}

#[tauri::command]
pub async fn check_local_cli_updates(
    state: State<'_, AppState>,
) -> Result<Vec<LocalCliTool>, String> {
    let mut tools = list_local_cli_tools(state.clone()).await?;
    let updater = LocalCliUpdater::new(Arc::clone(&state.db));
    updater
        .check_updates(&mut tools)
        .await
        .map_err(|e| e.to_string())?;
    Ok(tools)
}

#[tauri::command]
pub async fn update_local_cli_tool(
    state: State<'_, AppState>,
    tool_id: String,
) -> Result<String, String> {
    let row = state
        .db
        .get_local_cli_tool(&tool_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("工具 {} 未找到", tool_id))?;

    let (_, detected_path, manager_str, current_version, _, _, _, _, _, pkg_name, _desc) = row;
    let manager = PackageManager::from_str(&manager_str);
    let mut tool = LocalCliTool::new(&tool_id, &detected_path, manager);
    tool.current_version = current_version;
    tool.package_name = pkg_name;

    let (bin, args) = build_pty_update_args(&tool)
        .ok_or_else(|| format!("工具 {} 的包管理器不支持自动更新", tool_id))?;

    state
        .db
        .set_local_cli_tool_update_status(&tool_id, "updating", None)
        .map_err(|e| e.to_string())?;

    let cli = ClaudeCli::new(bin);
    let command = ClaudeCommand {
        args,
        timeout: Duration::from_secs(120),
    };

    let detected_path_clone = detected_path.clone();
    let manager_str_clone = manager_str.clone();
    let tool_id_clone = tool_id.clone();

    match tokio::task::spawn_blocking(move || cli.run(&[command])).await {
        Ok(Ok(result)) => {
            let log = result.raw_log.clone();
            if result.exit_success {
                state
                    .db
                    .set_local_cli_tool_update_status(&tool_id, "success", Some(&log))
                    .map_err(|e| e.to_string())?;
                let new_version = crate::services::local_cli_scanner::detect_version(
                    std::path::Path::new(&detected_path_clone),
                );
                let _ = state.db.upsert_local_cli_tool(
                    &tool_id_clone,
                    &detected_path_clone,
                    &manager_str_clone,
                    new_version.as_deref(),
                    None,
                    false,
                    None,
                    None,
                    None,
                );
                Ok(log)
            } else {
                state
                    .db
                    .set_local_cli_tool_update_status(&tool_id, "failed", Some(&log))
                    .map_err(|e| e.to_string())?;
                Err(log)
            }
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            state
                .db
                .set_local_cli_tool_update_status(&tool_id, "failed", Some(&msg))
                .map_err(|e| e.to_string())?;
            Err(msg)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn uninstall_local_cli_tool(
    state: State<'_, AppState>,
    tool_id: String,
) -> Result<String, String> {
    let row = state
        .db
        .get_local_cli_tool(&tool_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("工具 {} 未找到", tool_id))?;

    let (_, detected_path, manager_str, current_version, _, _, _, _, _, pkg_name, _desc) = row;
    let manager = PackageManager::from_str(&manager_str);
    let mut tool = LocalCliTool::new(&tool_id, &detected_path, manager);
    tool.current_version = current_version;
    tool.package_name = pkg_name;

    let (bin, args) = build_pty_uninstall_args(&tool)
        .ok_or_else(|| format!("工具 {} 的包管理器不支持自动卸载", tool_id))?;

    state
        .db
        .set_local_cli_tool_update_status(&tool_id, "uninstalling", None)
        .map_err(|e| e.to_string())?;

    let cli = ClaudeCli::new(bin);
    let command = ClaudeCommand {
        args,
        timeout: Duration::from_secs(120),
    };

    match tokio::task::spawn_blocking(move || cli.run(&[command])).await {
        Ok(Ok(result)) => {
            let log = result.raw_log.clone();
            if result.exit_success {
                state
                    .db
                    .delete_local_cli_tool(&tool_id)
                    .map_err(|e| e.to_string())?;
                Ok(log)
            } else {
                state
                    .db
                    .set_local_cli_tool_update_status(&tool_id, "failed", Some(&log))
                    .map_err(|e| e.to_string())?;
                Err(log)
            }
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            state
                .db
                .set_local_cli_tool_update_status(&tool_id, "failed", Some(&msg))
                .map_err(|e| e.to_string())?;
            Err(msg)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn open_local_cli_folder(
    state: State<'_, AppState>,
    tool_id: String,
) -> Result<(), String> {
    let row = state
        .db
        .get_local_cli_tool(&tool_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("工具 {} 未找到", tool_id))?;

    let detected_path = PathBuf::from(row.1);
    let folder = detected_path
        .parent()
        .ok_or_else(|| format!("无法获取工具 {} 的安装目录", tool_id))?;
    if !folder.exists() || !folder.is_dir() {
        return Err(format!("安装目录不存在: {}", folder.display()));
    }
    let canonical = folder
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;
    let canonical_str = canonical.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&canonical_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&canonical_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&canonical_str)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn fetch_local_cli_descriptions(
    state: State<'_, AppState>,
    tool_ids: Vec<String>,
) -> Result<Vec<(String, String)>, String> {
    let mut results = Vec::new();

    for tool_id in &tool_ids {
        if let Ok(Some(row)) = state.db.get_local_cli_tool(tool_id) {
            if row.10.is_some() {
                continue;
            }
        }

        let row = match state.db.get_local_cli_tool(tool_id) {
            Ok(Some(r)) => r,
            _ => continue,
        };
        let detected_path = row.1.clone();
        let id = tool_id.clone();

        let desc = tokio::task::spawn_blocking(move || {
            resolve_description_for_path(&PathBuf::from(detected_path))
        })
        .await
        .ok()
        .flatten();

        let Some(desc) = desc else { continue };
        let _ = state.db.set_local_cli_tool_description(&id, &desc);
        results.push((id, desc));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LocalCliTool, PackageManager};

    #[test]
    fn build_pty_args_for_npm() {
        let mut tool = LocalCliTool::new("mmdc", "/usr/bin/mmdc", PackageManager::Npm);
        tool.package_name = Some("@mermaid-js/mermaid-cli".to_string());
        let (bin, argv) = build_pty_update_args(&tool).unwrap();
        assert_eq!(bin, "npm");
        assert_eq!(argv, vec!["install", "-g", "@mermaid-js/mermaid-cli"]);
    }

    #[test]
    fn build_pty_args_for_pip() {
        let mut tool = LocalCliTool::new("bdc", "/home/u/.local/bin/bdc", PackageManager::Pip);
        tool.package_name = Some("bruce-doc-converter".to_string());
        let (bin, argv) = build_pty_update_args(&tool).unwrap();
        assert_eq!(bin, "pip");
        assert_eq!(argv, vec!["install", "--upgrade", "bruce-doc-converter"]);
    }

    #[test]
    fn build_pty_uninstall_args_for_supported_managers() {
        let cases = [
            (
                PackageManager::Npm,
                "npm",
                vec!["uninstall", "-g", "@mermaid-js/mermaid-cli"],
            ),
            (
                PackageManager::Pip,
                "pip",
                vec!["uninstall", "-y", "bruce-doc-converter"],
            ),
            (
                PackageManager::Brew,
                "brew",
                vec!["uninstall", "agent-skills-guard"],
            ),
            (
                PackageManager::Scoop,
                "scoop",
                vec!["uninstall", "agent-skills-guard"],
            ),
            (
                PackageManager::Choco,
                "choco",
                vec!["uninstall", "agent-skills-guard", "-y"],
            ),
        ];

        for (manager, expected_bin, expected_args) in cases {
            let mut tool = LocalCliTool::new("tool", "/usr/bin/tool", manager.clone());
            tool.package_name = Some(expected_args.last().unwrap().to_string());
            if matches!(manager, PackageManager::Npm) {
                tool.package_name = Some("@mermaid-js/mermaid-cli".to_string());
            }
            if matches!(manager, PackageManager::Pip) {
                tool.package_name = Some("bruce-doc-converter".to_string());
            }
            if matches!(manager, PackageManager::Choco) {
                tool.package_name = Some("agent-skills-guard".to_string());
            }

            let (bin, argv) = build_pty_uninstall_args(&tool).unwrap();
            assert_eq!(bin, expected_bin);
            assert_eq!(argv, expected_args);
        }
    }

    #[test]
    fn build_pty_args_returns_none_without_package_name() {
        let tool = LocalCliTool::new("mmdc", "/usr/bin/mmdc", PackageManager::Npm);
        assert!(build_pty_update_args(&tool).is_none());
    }

    #[test]
    fn build_pty_args_returns_none_for_unknown() {
        let mut tool = LocalCliTool::new("git", "/usr/bin/git", PackageManager::Unknown);
        tool.package_name = Some("git".to_string());
        assert!(build_pty_update_args(&tool).is_none());
    }
}
