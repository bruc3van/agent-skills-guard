use crate::commands::AppState;
use crate::models::{LocalCliTool, PackageManager};
use crate::services::claude_cli::{ClaudeCli, ClaudeCommand};
use crate::services::{discover_local_cli_tools, resolve_description_for_path, LocalCliUpdater};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

pub fn build_pty_update_args(tool: &LocalCliTool) -> Option<(String, Vec<String>)> {
    let pkg = tool.package_name.as_deref()?;
    let (bin, args) = match tool.manager {
        PackageManager::Npm => (
            resolve_package_manager_command(tool, &package_manager_names("npm")),
            vec!["install".to_string(), "-g".to_string(), pkg.to_string()],
        ),
        PackageManager::Pip => {
            let mut args = pip_prefix_args();
            args.extend([
                "install".to_string(),
                "--upgrade".to_string(),
                pkg.to_string(),
            ]);
            (resolve_python_command(tool), args)
        }
        PackageManager::Brew => (
            resolve_package_manager_command(tool, &package_manager_names("brew")),
            vec!["upgrade".to_string(), pkg.to_string()],
        ),
        PackageManager::Scoop => (
            resolve_package_manager_command(tool, &package_manager_names("scoop")),
            vec!["update".to_string(), pkg.to_string()],
        ),
        PackageManager::Choco => (
            resolve_package_manager_command(tool, &package_manager_names("choco")),
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
            resolve_package_manager_command(tool, &package_manager_names("npm")),
            vec!["uninstall".to_string(), "-g".to_string(), pkg.to_string()],
        ),
        PackageManager::Pip => {
            let mut args = pip_prefix_args();
            args.extend(["uninstall".to_string(), "-y".to_string(), pkg.to_string()]);
            (resolve_python_command(tool), args)
        }
        PackageManager::Brew => (
            resolve_package_manager_command(tool, &package_manager_names("brew")),
            vec!["uninstall".to_string(), pkg.to_string()],
        ),
        PackageManager::Scoop => (
            resolve_package_manager_command(tool, &package_manager_names("scoop")),
            vec!["uninstall".to_string(), pkg.to_string()],
        ),
        PackageManager::Choco => (
            resolve_package_manager_command(tool, &package_manager_names("choco")),
            vec!["uninstall".to_string(), pkg.to_string(), "-y".to_string()],
        ),
        PackageManager::Unknown => return None,
    };
    Some((bin.to_string(), args))
}

fn pip_prefix_args() -> Vec<String> {
    vec!["-m".to_string(), "pip".to_string()]
}

fn package_manager_names(base: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![
            format!("{}.cmd", base),
            format!("{}.exe", base),
            base.to_string(),
        ]
    } else {
        vec![base.to_string()]
    }
}

fn resolve_package_manager_command(tool: &LocalCliTool, names: &[String]) -> String {
    let detected_path = Path::new(&tool.detected_path);
    if let Some(path) = find_sibling_command(detected_path, names) {
        return path.to_string_lossy().to_string();
    }

    for path in common_package_manager_paths(names) {
        if path.is_file() {
            return path.to_string_lossy().to_string();
        }
    }

    names
        .last()
        .cloned()
        .unwrap_or_else(|| tool.manager.as_str().to_string())
}

fn resolve_python_command(tool: &LocalCliTool) -> String {
    let detected_path = Path::new(&tool.detected_path);
    let names = python_names();
    if let Some(path) = find_sibling_command(detected_path, &names) {
        return path.to_string_lossy().to_string();
    }

    if let Some(path) = find_python_next_to_scripts_dir(detected_path) {
        return path.to_string_lossy().to_string();
    }

    for path in common_python_paths() {
        if path.is_file() {
            return path.to_string_lossy().to_string();
        }
    }

    default_python_command(detected_path)
}

fn find_sibling_command(detected_path: &Path, names: &[String]) -> Option<PathBuf> {
    let parent = detected_path.parent()?;
    names
        .iter()
        .map(|name| parent.join(name))
        .find(|candidate| candidate.is_file())
}

fn find_python_next_to_scripts_dir(detected_path: &Path) -> Option<PathBuf> {
    let parent = detected_path.parent()?;
    let parent_name = parent
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !parent_name.eq_ignore_ascii_case("scripts") {
        return None;
    }
    let root = parent.parent()?;
    python_names()
        .iter()
        .map(|name| root.join(name))
        .find(|candidate| candidate.is_file())
}

fn python_names() -> Vec<String> {
    if cfg!(windows) {
        vec!["python.exe".to_string(), "python".to_string()]
    } else {
        vec!["python3".to_string(), "python".to_string()]
    }
}

fn common_package_manager_paths(names: &[String]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for prefix in common_bin_prefixes() {
        for name in names {
            paths.push(prefix.join(name));
        }
    }
    paths
}

fn common_python_paths() -> Vec<PathBuf> {
    common_bin_prefixes()
        .into_iter()
        .flat_map(|prefix| {
            python_names()
                .into_iter()
                .map(move |name| prefix.join(name))
        })
        .collect()
}

fn common_bin_prefixes() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]
}

fn default_python_command(detected_path: &Path) -> String {
    let path = detected_path.to_string_lossy();
    if cfg!(windows) && (path.contains('\\') || path.contains(':')) {
        "python".to_string()
    } else {
        "python3".to_string()
    }
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
        assert_eq!(bin, "python3");
        assert_eq!(
            argv,
            vec!["-m", "pip", "install", "--upgrade", "bruce-doc-converter"]
        );
    }

    #[test]
    fn build_pty_args_for_pip_uses_python_from_same_virtualenv() {
        let dir = tempfile::tempdir().unwrap();
        let env = dir.path().join("venv");
        let bin_dir = env.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let python = bin_dir.join("python");
        std::fs::write(&python, b"").unwrap();

        let mut tool = LocalCliTool::new(
            "bdc",
            &bin_dir.join("bdc").to_string_lossy(),
            PackageManager::Pip,
        );
        tool.package_name = Some("bruce-doc-converter".to_string());

        let (bin, argv) = build_pty_uninstall_args(&tool).unwrap();
        assert_eq!(bin, python.to_string_lossy());
        assert_eq!(
            argv,
            vec!["-m", "pip", "uninstall", "-y", "bruce-doc-converter"]
        );
    }

    #[test]
    fn build_pty_args_prefers_package_manager_next_to_detected_cli() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let npm = bin_dir.join(if cfg!(windows) { "npm.cmd" } else { "npm" });
        std::fs::write(&npm, b"").unwrap();

        let mut tool = LocalCliTool::new(
            "mmdc",
            &bin_dir.join("mmdc").to_string_lossy(),
            PackageManager::Npm,
        );
        tool.package_name = Some("@mermaid-js/mermaid-cli".to_string());

        let (bin, argv) = build_pty_update_args(&tool).unwrap();
        assert_eq!(bin, npm.to_string_lossy());
        assert_eq!(argv, vec!["install", "-g", "@mermaid-js/mermaid-cli"]);
    }

    #[test]
    fn build_pty_args_do_not_generate_sudo_commands() {
        let managers = [
            PackageManager::Npm,
            PackageManager::Pip,
            PackageManager::Brew,
            PackageManager::Scoop,
            PackageManager::Choco,
        ];

        for manager in managers {
            let mut tool = LocalCliTool::new("tool", "/usr/local/bin/tool", manager);
            tool.package_name = Some("tool".to_string());
            let (update_bin, update_args) = build_pty_update_args(&tool).unwrap();
            let (uninstall_bin, uninstall_args) = build_pty_uninstall_args(&tool).unwrap();
            assert_ne!(update_bin, "sudo");
            assert_ne!(uninstall_bin, "sudo");
            assert!(!update_args.iter().any(|arg| arg == "sudo"));
            assert!(!uninstall_args.iter().any(|arg| arg == "sudo"));
        }
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
                "python3",
                vec!["-m", "pip", "uninstall", "-y", "bruce-doc-converter"],
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
