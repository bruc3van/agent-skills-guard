use crate::commands::AppState;
use crate::models::{LocalCliTool, PackageManager};
use crate::services::claude_cli::{ClaudeCli, ClaudeCommand};
use crate::services::{discover_local_cli_tools, local_cli_updater::is_outdated, resolve_description_for_path, LocalCliUpdater};
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
        PackageManager::Pnpm => (
            resolve_package_manager_command(tool, &package_manager_names("pnpm")),
            vec!["add".to_string(), "-g".to_string(), pkg.to_string()],
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
        PackageManager::Pnpm => (
            resolve_package_manager_command(tool, &package_manager_names("pnpm")),
            vec!["remove".to_string(), "-g".to_string(), pkg.to_string()],
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

    if let Some(path) = find_python_from_script_shebang(detected_path) {
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

fn find_python_from_script_shebang(detected_path: &Path) -> Option<PathBuf> {
    let script = std::fs::canonicalize(detected_path).ok()?;
    let content = std::fs::read_to_string(script).ok()?;
    let first_line = content.lines().next()?.trim();
    let python_path = first_line.strip_prefix("#!")?.trim();
    let python_path = python_path.split_whitespace().next()?;
    if !looks_like_python_command(python_path) {
        return None;
    }
    let path = PathBuf::from(python_path);
    path.is_file().then_some(path)
}

fn looks_like_python_command(command: &str) -> bool {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("python"))
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

fn sanitize_terminal_log(raw: &str) -> String {
    let stripped = strip_ansi_sequences(raw);
    let rendered = render_terminal_line_controls(&stripped);

    rendered
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !is_progress_noise_line(line.trim()))
        .filter(|line| !is_ascii_art_line(line.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn non_empty_log_or_default(log: String, fallback: impl Into<String>) -> String {
    if log.trim().is_empty() {
        fallback.into()
    } else {
        log
    }
}

fn strip_ansi_sequences(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let chars = raw.chars().collect::<Vec<_>>();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c == '\u{1b}' {
            i += 1;
            if i >= chars.len() {
                break;
            }

            match chars[i] {
                '[' => {
                    i += 1;
                    while i < chars.len() {
                        let ch = chars[i];
                        i += 1;
                        if ('\u{40}'..='\u{7e}').contains(&ch) {
                            break;
                        }
                    }
                }
                ']' | 'P' | '^' | '_' | 'X' => {
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '\u{7}' {
                            i += 1;
                            break;
                        }
                        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                '(' | ')' | '*' | '+' | '-' | '.' | '/' => {
                    i += usize::min(2, chars.len().saturating_sub(i));
                }
                _ => {
                    i += 1;
                }
            }
            continue;
        }

        if c == '\u{9b}' {
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                i += 1;
                if ('\u{40}'..='\u{7e}').contains(&ch) {
                    break;
                }
            }
            continue;
        }

        out.push(c);
        i += 1;
    }

    out
}

fn render_terminal_line_controls(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut current_line = String::new();
    let chars = raw.chars().collect::<Vec<_>>();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\r' => {
                if i + 1 < chars.len() && chars[i + 1] == '\n' {
                    out.push_str(&current_line);
                    out.push('\n');
                    current_line.clear();
                    i += 2;
                    continue;
                }
                current_line.clear();
            }
            '\n' => {
                out.push_str(&current_line);
                out.push('\n');
                current_line.clear();
            }
            '\u{8}' => {
                current_line.pop();
            }
            '\t' => current_line.push('\t'),
            c if c.is_control() => {}
            c => current_line.push(c),
        }
        i += 1;
    }

    out.push_str(&current_line);
    out
}

fn is_progress_noise_line(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return true;
    };

    chars.next().is_none()
        && matches!(
            first as u32,
            0x280b
                | 0x2819
                | 0x2839
                | 0x2838
                | 0x283c
                | 0x2834
                | 0x2826
                | 0x2827
                | 0x2807
                | 0x280f
                | 0x2d
                | 0x5c
                | 0x7c
                | 0x2f
        )
}

fn is_ascii_art_line(line: &str) -> bool {
    let mut total = 0;
    let mut visual = 0;
    let mut alphanumeric = 0;

    for ch in line.chars().filter(|ch| !ch.is_whitespace()) {
        total += 1;
        if ch.is_alphanumeric() {
            alphanumeric += 1;
        }
        if is_ascii_art_char(ch) {
            visual += 1;
        }
    }

    total >= 8 && visual * 100 / total >= 45 && alphanumeric * 100 / total <= 55
}

fn is_ascii_art_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x2500..=0x257f
            | 0x2580..=0x259f
            | 0x25a0..=0x25ff
            | 0x2800..=0x28ff
            | 0x2b00..=0x2bff
            | 0x2f
            | 0x5c
            | 0x7c
            | 0x5f
            | 0x3d
            | 0x23
            | 0x40
            | 0x2a
            | 0x7e
            | 0x5e
            | 0x2b
    )
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
                (path, (id, mgr, cur, lat, upd, chk, status, log, pkg, desc))
            },
        )
        .collect();

    for tool in tools.iter_mut() {
        if let Some((_, _, _, latest, _update_avail, checked, status, log, _pkg, desc)) =
            cache_map.get(&tool.detected_path)
        {
            tool.latest_version =
                latest.as_deref().map(|v| v.strip_prefix('v').unwrap_or(v).to_string());
            tool.update_available = is_outdated(
                tool.current_version.as_deref(),
                tool.latest_version.as_deref(),
            );
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
    tool_path: String,
) -> Result<String, String> {
    let row = state
        .db
        .get_local_cli_tool(&tool_path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("工具 {} 未找到", tool_path))?;

    let (display_id, detected_path, manager_str, current_version, _, _, _, _, _, pkg_name, _desc) =
        row;
    let manager = PackageManager::from_str(&manager_str);
    let mut tool = LocalCliTool::new(&display_id, &detected_path, manager);
    tool.current_version = current_version;
    tool.package_name = pkg_name;

    let (bin, args) = build_pty_update_args(&tool)
        .ok_or_else(|| format!("工具 {} 的包管理器不支持自动更新", display_id))?;

    state
        .db
        .set_local_cli_tool_update_status(&tool_path, "updating", None)
        .map_err(|e| e.to_string())?;

    let timeout = Duration::from_secs(update_timeout_secs(&tool.manager));
    let cli = build_cli_for_manager(bin, &tool.manager);
    let command = ClaudeCommand { args, timeout };

    let detected_path_clone = detected_path.clone();
    let manager_str_clone = manager_str.clone();
    let display_id_clone = display_id.clone();

    match tokio::task::spawn_blocking(move || cli.run(&[command])).await {
        Ok(Ok(result)) => {
            let raw_log = sanitize_terminal_log(&result.raw_log);
            let is_up_to_date = !result.exit_success && is_already_up_to_date(&raw_log);
            let install_ok = !result.exit_success && is_install_success(&raw_log);
            if result.exit_success || is_up_to_date || install_ok {
                let log = non_empty_log_or_default(
                    raw_log,
                    format!("{} 更新命令执行完成，但没有返回可显示日志", display_id),
                );
                state
                    .db
                    .set_local_cli_tool_update_status(&tool_path, "success", Some(&log))
                    .map_err(|e| e.to_string())?;
                let new_version = crate::services::local_cli_scanner::detect_version(
                    std::path::Path::new(&detected_path_clone),
                );
                let _ = state.db.upsert_local_cli_tool(
                    &display_id_clone,
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
                let log = non_empty_log_or_default(
                    raw_log,
                    format!("{} 更新命令执行失败，但没有返回可显示日志", display_id),
                );
                state
                    .db
                    .set_local_cli_tool_update_status(&tool_path, "failed", Some(&log))
                    .map_err(|e| e.to_string())?;
                Err(log)
            }
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            state
                .db
                .set_local_cli_tool_update_status(&tool_path, "failed", Some(&msg))
                .map_err(|e| e.to_string())?;
            Err(msg)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn build_cli_for_manager(bin: String, manager: &PackageManager) -> ClaudeCli {
    let mut cli = ClaudeCli::new(bin);
    match manager {
        PackageManager::Npm | PackageManager::Pnpm => {
            cli = cli.env_remove_prefix("npm_config_");
        }
        PackageManager::Brew => {
            cli = cli
                .env_var("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env_var("HOMEBREW_NO_EMOJI", "1");
        }
        _ => {}
    }
    cli
}

fn is_already_up_to_date(output: &str) -> bool {
    let text = output.to_lowercase();
    text.contains("already installed")
        || text.contains("up to date")
        || text.contains("already up-to-date")
        || text.contains("already satisfied")
        || text.contains("is already the latest version")
        || text.contains("is the latest version")
}

fn is_install_success(output: &str) -> bool {
    let text = output.to_lowercase();
    (text.contains("added") || text.contains("changed") || text.contains("removed"))
        && text.contains("package")
        && text.contains("in ")
}

fn update_timeout_secs(manager: &PackageManager) -> u64 {
    match manager {
        PackageManager::Brew => 300,
        _ => 120,
    }
}

#[tauri::command]
pub async fn uninstall_local_cli_tool(
    state: State<'_, AppState>,
    tool_path: String,
) -> Result<String, String> {
    let row = state
        .db
        .get_local_cli_tool(&tool_path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("工具 {} 未找到", tool_path))?;

    let (display_id, detected_path, manager_str, current_version, _, _, _, _, _, pkg_name, _desc) =
        row;
    let manager = PackageManager::from_str(&manager_str);
    let mut tool = LocalCliTool::new(&display_id, &detected_path, manager);
    tool.current_version = current_version;
    tool.package_name = pkg_name;

    let (bin, args) = build_pty_uninstall_args(&tool)
        .ok_or_else(|| format!("工具 {} 的包管理器不支持自动卸载", display_id))?;

    state
        .db
        .set_local_cli_tool_update_status(&tool_path, "uninstalling", None)
        .map_err(|e| e.to_string())?;

    let timeout = Duration::from_secs(update_timeout_secs(&tool.manager));
    let cli = build_cli_for_manager(bin, &tool.manager);
    let command = ClaudeCommand { args, timeout };

    match tokio::task::spawn_blocking(move || cli.run(&[command])).await {
        Ok(Ok(result)) => {
            let raw_log = sanitize_terminal_log(&result.raw_log);
            if result.exit_success {
                let log = non_empty_log_or_default(
                    raw_log,
                    format!("{} 卸载命令执行完成，但没有返回可显示日志", display_id),
                );
                state
                    .db
                    .delete_local_cli_tool(&tool_path)
                    .map_err(|e| e.to_string())?;
                Ok(log)
            } else {
                let log = non_empty_log_or_default(
                    raw_log,
                    format!("{} 卸载命令执行失败，但没有返回可显示日志", display_id),
                );
                state
                    .db
                    .set_local_cli_tool_update_status(&tool_path, "failed", Some(&log))
                    .map_err(|e| e.to_string())?;
                Err(log)
            }
        }
        Ok(Err(e)) => {
            let msg = e.to_string();
            state
                .db
                .set_local_cli_tool_update_status(&tool_path, "failed", Some(&msg))
                .map_err(|e| e.to_string())?;
            Err(msg)
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn open_local_cli_folder(
    state: State<'_, AppState>,
    tool_path: String,
) -> Result<(), String> {
    let row = state
        .db
        .get_local_cli_tool(&tool_path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("工具 {} 未找到", tool_path))?;

    let detected_path = PathBuf::from(row.1);
    let folder = detected_path
        .parent()
        .ok_or_else(|| format!("无法获取工具 {} 的安装目录", tool_path))?;
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
    tool_paths: Vec<String>,
) -> Result<Vec<(String, String)>, String> {
    let mut results = Vec::new();

    for tool_path in &tool_paths {
        let row = match state.db.get_local_cli_tool(tool_path) {
            Ok(Some(r)) if r.10.is_some() => continue,
            Ok(Some(r)) => r,
            _ => continue,
        };
        let detected_path = row.1.clone();
        let path = tool_path.clone();

        let desc = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                resolve_description_for_path(&PathBuf::from(detected_path))
            }),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten();

        let Some(desc) = desc else { continue };
        let _ = state.db.set_local_cli_tool_description(&path, &desc);
        results.push((path, desc));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LocalCliTool, PackageManager};
    use std::path::Path;

    fn command_name(path_or_name: &str) -> String {
        Path::new(path_or_name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path_or_name)
            .to_string()
    }

    #[test]
    fn build_pty_args_for_npm() {
        let mut tool = LocalCliTool::new("mmdc", "/usr/bin/mmdc", PackageManager::Npm);
        tool.package_name = Some("@mermaid-js/mermaid-cli".to_string());
        let (bin, argv) = build_pty_update_args(&tool).unwrap();
        assert_eq!(command_name(&bin), "npm");
        assert_eq!(argv, vec!["install", "-g", "@mermaid-js/mermaid-cli"]);
    }

    #[test]
    fn build_pty_args_for_pnpm() {
        let mut tool = LocalCliTool::new(
            "mmdc",
            "/Users/u/Library/pnpm/bin/mmdc",
            PackageManager::Pnpm,
        );
        tool.package_name = Some("@mermaid-js/mermaid-cli".to_string());
        let (bin, argv) = build_pty_update_args(&tool).unwrap();
        assert_eq!(command_name(&bin), "pnpm");
        assert_eq!(argv, vec!["add", "-g", "@mermaid-js/mermaid-cli"]);

        let (bin, argv) = build_pty_uninstall_args(&tool).unwrap();
        assert_eq!(command_name(&bin), "pnpm");
        assert_eq!(argv, vec!["remove", "-g", "@mermaid-js/mermaid-cli"]);
    }

    #[test]
    fn build_pty_args_for_pip() {
        let mut tool = LocalCliTool::new("bdc", "/home/u/.local/bin/bdc", PackageManager::Pip);
        tool.package_name = Some("bruce-doc-converter".to_string());
        let (bin, argv) = build_pty_update_args(&tool).unwrap();
        assert_eq!(command_name(&bin), "python3");
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
    fn build_pty_args_for_pip_uses_python_from_script_shebang() {
        let dir = tempfile::tempdir().unwrap();
        let venv_bin = dir
            .path()
            .join("pipx")
            .join("venvs")
            .join("markitdown")
            .join("bin");
        let user_bin = dir.path().join(".local").join("bin");
        std::fs::create_dir_all(&venv_bin).unwrap();
        std::fs::create_dir_all(&user_bin).unwrap();
        let python = venv_bin.join("python");
        let script_target = venv_bin.join("markitdown");
        let detected_script = user_bin.join("markitdown");
        std::fs::write(&python, b"").unwrap();
        std::fs::write(
            &script_target,
            format!("#!{}\nimport sys\n", python.to_string_lossy()),
        )
        .unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&script_target, &detected_script).unwrap();
        #[cfg(windows)]
        {
            if let Err(err) = std::os::windows::fs::symlink_file(&script_target, &detected_script) {
                if err.raw_os_error() == Some(1314) {
                    std::fs::write(
                        &detected_script,
                        format!("#!{}\nimport sys\n", python.to_string_lossy()),
                    )
                    .unwrap();
                } else {
                    panic!("failed to create symlink: {err}");
                }
            }
        }

        let mut tool = LocalCliTool::new(
            "markitdown",
            &detected_script.to_string_lossy(),
            PackageManager::Pip,
        );
        tool.package_name = Some("markitdown".to_string());

        let (bin, argv) = build_pty_uninstall_args(&tool).unwrap();

        assert_eq!(bin, python.to_string_lossy());
        assert_eq!(argv, vec!["-m", "pip", "uninstall", "-y", "markitdown"]);
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
            PackageManager::Pnpm,
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
                PackageManager::Pnpm,
                "pnpm",
                vec!["remove", "-g", "@mermaid-js/mermaid-cli"],
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
            if matches!(manager, PackageManager::Pnpm) {
                tool.package_name = Some("@mermaid-js/mermaid-cli".to_string());
            }
            if matches!(manager, PackageManager::Pip) {
                tool.package_name = Some("bruce-doc-converter".to_string());
            }
            if matches!(manager, PackageManager::Choco) {
                tool.package_name = Some("agent-skills-guard".to_string());
            }

            let (bin, argv) = build_pty_uninstall_args(&tool).unwrap();
            assert_eq!(command_name(&bin), expected_bin);
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

    #[test]
    fn sanitizes_npm_pty_log() {
        let raw = "\u{1b}[?9001h\u{1b}[?1004h\u{1b}[?25l\u{1b}[2J\u{1b}[m\u{1b}[H\u{1b}]0;npm\u{7}\u{1b}[?25h\u{1b}[1mnpm\u{1b}[22m \u{1b}[33mwarn \u{1b}[94mUnknown env config \"_jsr-registry\". This will stop working in the next major version of npm.\r\n\u{1b}]0;npm install @openai/codex\u{7}\u{1b}[m⠙\u{1b}[K\r⠹\u{1b}[K\r\u{1b}[Kadded 1 package in 2s\r\n";

        let log = sanitize_terminal_log(raw);

        assert!(!log.contains('\u{1b}'));
        assert!(!log.contains("]0;"));
        assert!(!log.contains('⠙'));
        assert!(log.contains(
            "npm warn Unknown env config \"_jsr-registry\". This will stop working in the next major version of npm."
        ));
        assert!(log.contains("added 1 package in 2s"));
    }

    #[test]
    fn sanitizes_carriage_return_progress_without_dropping_crlf_lines() {
        let raw = "first line\r\nInstalling 1%\rInstalling 100%\r\nDone\r\n";

        assert_eq!(
            sanitize_terminal_log(raw),
            "first line\nInstalling 100%\nDone"
        );
    }

    #[test]
    fn removes_block_character_banner_lines() {
        let raw = "opencode\r\n█▀▀█ █▀▀█ █▀▀█ █▀▀▄ █▀▀▀ █▀▀█ █▀▀█ █▀▀█\r\nopencode installed\r\n";

        let log = sanitize_terminal_log(raw);

        assert_eq!(log, "opencode\nopencode installed");
    }

    #[test]
    fn install_success_patterns() {
        assert!(is_install_success("changed 1 package in 585ms"));
        assert!(is_install_success(
            "added 2 packages, removed 24 packages, and changed 295 packages in 7s"
        ));
        assert!(is_install_success("removed 2 packages, and changed 28 packages in 1s"));
        assert!(is_install_success("added 1 package in 2s"));
        assert!(!is_install_success("npm ERR! code E404"));
        assert!(!is_install_success("up to date"));
    }
}
