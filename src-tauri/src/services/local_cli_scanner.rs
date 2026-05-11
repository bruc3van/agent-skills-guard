use crate::models::{detect_manager_from_path, LocalCliTool};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;

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
                .output()
                .ok()?
        } else {
            Command::new(path).arg("--version").output().ok()?
        }
    };
    #[cfg(not(windows))]
    let output = Command::new(path).arg("--version").output().ok()?;

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
            let mut tool = LocalCliTool::new(&id, &path.to_string_lossy(), manager);
            tool.current_version = detect_version(path);
            tool
        })
        .collect()
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
