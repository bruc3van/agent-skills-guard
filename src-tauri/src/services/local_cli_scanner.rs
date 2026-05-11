use crate::models::{detect_manager_from_path, LocalCliTool, PackageManager};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn tool_id_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .unwrap_or_else(|| path.file_name().unwrap_or_default());
    stem.to_string_lossy().to_lowercase()
}

pub fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(windows)]
    {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        matches!(ext.as_str(), "exe" | "cmd" | "bat" | "ps1")
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
}

pub fn scan_dir_for_executables(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return vec![];
    }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_executable(p))
        .collect()
}

pub fn scan_path_for_executables() -> Vec<PathBuf> {
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();

    let mut result = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for dir in path_dirs {
        for bin in scan_dir_for_executables(&dir) {
            let id = tool_id_from_path(&bin);
            if seen_ids.insert(id) {
                result.push(bin);
            }
        }
    }

    result
}

pub fn parse_version(output: &str) -> Option<String> {
    // Optional "v" prefix then semver triple — use a capture group to exclude the "v"
    let re = Regex::new(r"v?(\d+\.\d+\.\d+)").ok()?;
    re.captures(output)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

pub fn detect_version(path: &Path) -> Option<String> {
    let path_buf = path.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run_version_command(&path_buf));
    });
    rx.recv_timeout(Duration::from_secs(3))
        .ok()
        .flatten()
}

fn run_version_command(path: &Path) -> Option<String> {
    #[cfg(windows)]
    let output = {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext == "cmd" || ext == "bat" {
            let comspec =
                std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into());
            Command::new(comspec)
                .args(["/d", "/c", &path.to_string_lossy(), "--version"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .ok()?
        } else {
            Command::new(path)
                .arg("--version")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .ok()?
        }
    };
    #[cfg(not(windows))]
    let output = Command::new(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_version(&text)
}

pub fn discover_local_cli_tools() -> Vec<LocalCliTool> {
    use rayon::prelude::*;

    let bins = scan_path_for_executables();

    bins.par_iter()
        .map(|path| {
            let id = tool_id_from_path(path);
            let manager = detect_manager_from_path(path);
            let package_name = resolve_package_name(path, &manager);
            let mut tool = LocalCliTool::new(&id, &path.to_string_lossy(), manager);
            tool.current_version = detect_version(path);
            tool.package_name = package_name;
            tool
        })
        .collect()
}

fn resolve_package_name(path: &Path, manager: &PackageManager) -> Option<String> {
    match manager {
        PackageManager::Npm => resolve_npm_package_name(path),
        PackageManager::Pip => resolve_pip_package_name(path),
        PackageManager::Brew | PackageManager::Scoop | PackageManager::Choco => {
            Some(tool_id_from_path(path))
        }
        PackageManager::Unknown => None,
    }
}

fn resolve_npm_package_name(path: &Path) -> Option<String> {
    let s = path.to_string_lossy().to_lowercase().replace('\\', "/");
    let npm_global = if let Some(pos) = s.find("/npm/") {
        &s[..pos + 5]
    } else {
        return Some(tool_id_from_path(path));
    };

    let id = tool_id_from_path(path);
    let pkg_json_path = PathBuf::from(format!("{}/node_modules/{}/package.json", npm_global, id));
    if pkg_json_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&pkg_json_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(name) = json["name"].as_str() {
                    return Some(name.to_string());
                }
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(content) = read_npm_shim_content(path) {
            if let Some(name) = extract_npm_package_from_shim(&content) {
                return Some(name);
            }
        }
    }

    Some(id)
}

#[cfg(windows)]
fn read_npm_shim_content(path: &Path) -> Option<String> {
    let ext = path.extension().and_then(|e| e.to_str())?.to_lowercase();
    if ext == "cmd" || ext == "bat" {
        std::fs::read_to_string(path).ok()
    } else {
        None
    }
}

#[cfg(windows)]
fn extract_npm_package_from_shim(content: &str) -> Option<String> {
    let re = Regex::new(r#"node_modules[/\\](@?[^/\\]+[/\\][^/\\]+|[^/\\]+)[/\\]"#).ok()?;
    re.captures(content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().replace('\\', "/"))
}

fn resolve_pip_package_name(path: &Path) -> Option<String> {
    let s = path.to_string_lossy().to_lowercase().replace('\\', "/");
    let id = tool_id_from_path(path);

    let parent = if let Some(pos) = s.rfind('/') {
        &s[..pos]
    } else {
        return Some(id);
    };
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".dist-info") || name.ends_with(".egg-info") {
                let metadata_path = entry.path().join("METADATA");
                if let Ok(meta) = std::fs::read_to_string(&metadata_path) {
                    for line in meta.lines() {
                        if let Some(name_val) = line.strip_prefix("Name: ") {
                            let pkg_name = name_val.trim().to_string();
                            if !pkg_name.is_empty() {
                                return Some(pkg_name);
                            }
                        }
                    }
                }
                let record_path = entry.path().join("RECORD");
                if let Ok(record) = std::fs::read_to_string(&record_path) {
                    for line in record.lines() {
                        let line_lower = line.to_lowercase();
                        if line_lower.contains(&format!("{}.py", id))
                            || line_lower.contains(&format!("{}/__main__.py", id))
                        {
                            let dist_name = name
                                .trim_end_matches(".dist-info")
                                .trim_end_matches(".egg-info");
                            if let Some(dash) = dist_name.rfind('-') {
                                return Some(dist_name[..dash].to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_dir_finds_executables() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("my-tool");
        fs::write(&bin, b"#!/bin/sh\necho hello").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let found = scan_dir_for_executables(dir.path());
        #[cfg(unix)]
        assert!(found.iter().any(|p| p.file_name().unwrap() == "my-tool"));
        // On Windows the file has no .exe/.cmd extension so won't be found — that's expected
        let _ = found;
    }

    #[test]
    fn parse_version_from_various_outputs() {
        assert_eq!(
            parse_version("bruce-doc-converter 0.3.1"),
            Some("0.3.1".to_string())
        );
        assert_eq!(parse_version("1.2.3\n"), Some("1.2.3".to_string()));
        assert_eq!(parse_version("v2.0.0-beta.1"), Some("2.0.0".to_string()));
        assert_eq!(parse_version("usage: tool [options]"), None);
    }

    #[test]
    fn tool_id_strips_extension_on_windows() {
        assert_eq!(
            tool_id_from_path(std::path::Path::new("bruce-doc-converter.cmd")),
            "bruce-doc-converter"
        );
        assert_eq!(
            tool_id_from_path(std::path::Path::new("pandoc.exe")),
            "pandoc"
        );
        assert_eq!(
            tool_id_from_path(std::path::Path::new("mmdc")),
            "mmdc"
        );
    }
}
