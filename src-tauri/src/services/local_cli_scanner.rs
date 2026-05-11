use crate::models::{detect_manager_from_path, LocalCliTool, PackageManager};
use regex::Regex;
use std::path::{Path, PathBuf};

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
        matches!(ext.as_str(), "exe" | "cmd")
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
}

pub fn is_supported_cli_path(path: &Path) -> bool {
    detect_manager_from_path(path) != PackageManager::Unknown
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

    scan_path_dirs_for_supported_executables(path_dirs)
}

pub fn scan_path_dirs_for_supported_executables(path_dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for dir in path_dirs {
        for bin in scan_dir_for_executables(&dir) {
            if !is_supported_cli_path(&bin) {
                continue;
            }
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
    match detect_manager_from_path(path) {
        PackageManager::Npm => detect_npm_version(path),
        PackageManager::Pip => detect_pip_version(path),
        PackageManager::Brew
        | PackageManager::Scoop
        | PackageManager::Choco
        | PackageManager::Unknown => None,
    }
}

pub fn discover_local_cli_tools() -> Vec<LocalCliTool> {
    use rayon::prelude::*;

    let bins = scan_path_for_executables();

    bins.par_iter()
        .map(|path| {
            let id = tool_id_from_path(path);
            let manager = detect_manager_from_path(path);
            let package_name = resolve_package_name(path, &manager);
            let description = resolve_description(path, &manager);
            let mut tool = LocalCliTool::new(&id, &path.to_string_lossy(), manager);
            tool.current_version = detect_version(path);
            tool.package_name = package_name;
            tool.description = description;
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

fn resolve_description(path: &Path, manager: &PackageManager) -> Option<String> {
    match manager {
        PackageManager::Npm => resolve_npm_description(path),
        PackageManager::Pip => resolve_pip_description(path),
        _ => None,
    }
}

pub fn resolve_description_for_path(path: &Path) -> Option<String> {
    let manager = detect_manager_from_path(path);
    resolve_description(path, &manager)
}

fn resolve_npm_description(path: &Path) -> Option<String> {
    let npm_global = npm_global_root(path)?;
    let package_name = resolve_npm_package_name(path)?;
    let pkg_json_path = npm_package_json_path(&npm_global, &package_name);
    read_package_json_field(&pkg_json_path, "description")
}

fn resolve_pip_description(path: &Path) -> Option<String> {
    let id = tool_id_from_path(path);
    for dist_info in pip_metadata_dirs_for_script(path) {
        if !pip_dist_matches_script(&dist_info, &id) {
            continue;
        }
        if let Some(summary) = read_metadata_field(&dist_info.join("METADATA"), "Summary") {
            return Some(summary);
        }
    }
    None
}

fn resolve_npm_package_name(path: &Path) -> Option<String> {
    let npm_global = if let Some(root) = npm_global_root(path) {
        root
    } else {
        return Some(tool_id_from_path(path));
    };

    let id = tool_id_from_path(path);
    let pkg_json_path = npm_package_json_path(&npm_global, &id);
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

fn npm_global_root(path: &Path) -> Option<PathBuf> {
    let s = path.to_string_lossy().replace('\\', "/");
    let lower = s.to_lowercase();
    lower
        .find("/npm/")
        .map(|pos| PathBuf::from(s[..pos + 5].to_string()))
}

fn npm_package_json_path(npm_global: &Path, package_name: &str) -> PathBuf {
    package_name
        .split('/')
        .fold(npm_global.join("node_modules"), |acc, part| acc.join(part))
        .join("package.json")
}

fn read_package_json_field(path: &Path, field: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    json[field].as_str().map(|value| value.to_string())
}

fn detect_npm_version(path: &Path) -> Option<String> {
    let npm_global = npm_global_root(path)?;
    let package_name = resolve_npm_package_name(path)?;
    read_package_json_field(
        &npm_package_json_path(&npm_global, &package_name),
        "version",
    )
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
    let id = tool_id_from_path(path);
    for dist_info in pip_metadata_dirs_for_script(path) {
        if !pip_dist_matches_script(&dist_info, &id) {
            continue;
        }

        if let Some(name) = read_metadata_field(&dist_info.join("METADATA"), "Name") {
            return Some(name);
        }

        if let Some(name) = pip_dist_name_from_dir(&dist_info) {
            return Some(name);
        }
    }

    Some(id)
}

fn pip_metadata_roots_for_script(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else {
        return vec![];
    };

    let mut roots = vec![parent.to_path_buf()];

    let parent_name = parent
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if parent_name.eq_ignore_ascii_case("scripts") {
        if let Some(python_root) = parent.parent() {
            roots.push(python_root.join("Lib").join("site-packages"));
            roots.push(python_root.join("lib").join("site-packages"));
        }
    }

    if parent_name.eq_ignore_ascii_case("bin") {
        if let Some(env_root) = parent.parent() {
            let lib_root = env_root.join("lib");
            if let Ok(entries) = std::fs::read_dir(&lib_root) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.starts_with("python") {
                        roots.push(entry.path().join("site-packages"));
                    }
                }
            }
        }
    }

    roots
}

fn pip_metadata_dirs_for_script(path: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in pip_metadata_roots_for_script(path) {
        if !root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if (name.ends_with(".dist-info") || name.ends_with(".egg-info"))
                && seen.insert(path.clone())
            {
                dirs.push(path);
            }
        }
    }

    dirs
}

fn pip_dist_matches_script(dist_info: &Path, id: &str) -> bool {
    if let Ok(record) = std::fs::read_to_string(dist_info.join("RECORD")) {
        if record
            .lines()
            .any(|line| pip_record_line_references_script(line, id))
        {
            return true;
        }
    }

    if let Ok(entry_points) = std::fs::read_to_string(dist_info.join("entry_points.txt")) {
        if entry_points
            .lines()
            .any(|line| entry_point_line_references_script(line, id))
        {
            return true;
        }
    }

    false
}

fn pip_record_line_references_script(line: &str, id: &str) -> bool {
    let record_path = line
        .split(',')
        .next()
        .unwrap_or_default()
        .replace('\\', "/")
        .to_lowercase();
    let file_name = record_path.rsplit('/').next().unwrap_or_default();
    let id = id.to_lowercase();

    file_name == id
        || file_name == format!("{}.exe", id)
        || file_name == format!("{}.cmd", id)
        || file_name == format!("{}.py", id)
        || record_path.ends_with(&format!("{}/__main__.py", id))
}

fn entry_point_line_references_script(line: &str, id: &str) -> bool {
    let line = line.trim().to_lowercase();
    let id = id.to_lowercase();
    line.strip_prefix(&id)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

fn read_metadata_field(metadata_path: &Path, field: &str) -> Option<String> {
    let meta = std::fs::read_to_string(metadata_path).ok()?;
    let prefix = format!("{}: ", field);
    meta.lines().find_map(|line| {
        line.strip_prefix(&prefix).and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    })
}

fn pip_dist_name_from_dir(dist_info: &Path) -> Option<String> {
    let file_name = dist_info.file_name()?.to_string_lossy();
    let dist_name = file_name
        .trim_end_matches(".dist-info")
        .trim_end_matches(".egg-info");
    dist_name
        .rfind('-')
        .map(|dash| dist_name[..dash].to_string())
}

fn detect_pip_version(path: &Path) -> Option<String> {
    let id = tool_id_from_path(path);
    for dist_info in pip_metadata_dirs_for_script(path) {
        if !pip_dist_matches_script(&dist_info, &id) {
            continue;
        }
        if let Some(version) = read_metadata_field(&dist_info.join("METADATA"), "Version") {
            return Some(version);
        }
    }

    None
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
        assert_eq!(tool_id_from_path(std::path::Path::new("mmdc")), "mmdc");
    }

    #[test]
    fn supported_cli_path_rejects_windows_system_binaries() {
        let path = std::path::Path::new(r"C:\Windows\System32\WerFault.exe");
        assert!(!is_supported_cli_path(path));
    }

    #[test]
    fn scan_path_only_returns_supported_cli_locations() {
        let dir = tempfile::tempdir().unwrap();
        let unsupported = dir.path().join("WerFault.exe");
        fs::write(&unsupported, b"").unwrap();

        let supported_dir = tempfile::tempdir().unwrap();
        let supported_root = supported_dir
            .path()
            .join("AppData")
            .join("Roaming")
            .join("npm");
        fs::create_dir_all(&supported_root).unwrap();
        let supported = supported_root.join("bruce-doc-converter.cmd");
        fs::write(&supported, b"@echo off\r\necho 1.0.0\r\n").unwrap();

        let found = scan_path_dirs_for_supported_executables(vec![
            dir.path().to_path_buf(),
            supported_root.clone(),
        ]);

        assert_eq!(found, vec![supported]);
    }

    #[cfg(windows)]
    #[test]
    fn detect_version_does_not_execute_windows_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let npm_root = dir.path().join("AppData").join("Roaming").join("npm");
        fs::create_dir_all(&npm_root).unwrap();
        let marker = dir.path().join("executed.txt");
        let shim = npm_root.join("suspicious.cmd");
        fs::write(
            &shim,
            format!(
                "@echo off\r\necho executed > \"{}\"\r\necho 9.9.9\r\n",
                marker.display()
            ),
        )
        .unwrap();

        assert_eq!(detect_version(&shim), None);
        assert!(!marker.exists());
    }

    #[cfg(windows)]
    #[test]
    fn resolve_description_does_not_execute_windows_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        let npm_root = dir.path().join("AppData").join("Roaming").join("npm");
        fs::create_dir_all(&npm_root).unwrap();
        let marker = dir.path().join("executed.txt");
        let shim = npm_root.join("suspicious.cmd");
        fs::write(
            &shim,
            format!(
                "@echo off\r\necho executed > \"{}\"\r\necho dangerous description\r\n",
                marker.display()
            ),
        )
        .unwrap();

        assert_eq!(resolve_description_for_path(&shim), None);
        assert!(!marker.exists());
    }

    #[test]
    fn detect_version_reads_npm_package_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let npm_root = dir.path().join("AppData").join("Roaming").join("npm");
        let package_root = npm_root.join("node_modules").join("my-tool");
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            r#"{"name":"my-tool","version":"1.2.3"}"#,
        )
        .unwrap();
        let shim = npm_root.join("my-tool.cmd");
        fs::write(&shim, b"@echo off\r\necho should-not-run\r\n").unwrap();

        assert_eq!(detect_version(&shim), Some("1.2.3".to_string()));
    }

    #[test]
    fn detect_version_reads_pip_package_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let scripts = dir.path().join("Python311").join("Scripts");
        let dist_info = scripts.join("my_tool-4.5.6.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(scripts.join("my-tool.exe"), b"").unwrap();
        fs::write(
            dist_info.join("METADATA"),
            "Name: my-tool\nVersion: 4.5.6\n",
        )
        .unwrap();
        fs::write(dist_info.join("RECORD"), "my-tool.exe,,\n").unwrap();

        assert_eq!(
            detect_version(&scripts.join("my-tool.exe")),
            Some("4.5.6".to_string())
        );
    }

    #[test]
    fn detect_version_reads_pip_metadata_from_python_site_packages() {
        let dir = tempfile::tempdir().unwrap();
        let python_root = dir.path().join("Python314");
        let scripts = python_root.join("Scripts");
        let site_packages = python_root.join("Lib").join("site-packages");
        let dist_info = site_packages.join("bruce_doc_converter-0.1.2.dist-info");
        fs::create_dir_all(&scripts).unwrap();
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(scripts.join("bdc.exe"), b"").unwrap();
        fs::write(
            dist_info.join("METADATA"),
            "Name: bruce-doc-converter\nVersion: 0.1.2\n",
        )
        .unwrap();
        fs::write(dist_info.join("RECORD"), "../../Scripts/bdc.exe,,\n").unwrap();
        fs::write(
            dist_info.join("entry_points.txt"),
            "[console_scripts]\nbdc = bruce_doc_converter.cli:main\n",
        )
        .unwrap();

        assert_eq!(
            detect_version(&scripts.join("bdc.exe")),
            Some("0.1.2".to_string())
        );
    }

    #[test]
    fn resolve_pip_package_name_reads_console_script_owner() {
        let dir = tempfile::tempdir().unwrap();
        let python_root = dir.path().join("Python314");
        let scripts = python_root.join("Scripts");
        let site_packages = python_root.join("Lib").join("site-packages");
        let dist_info = site_packages.join("bruce_doc_converter-0.1.2.dist-info");
        fs::create_dir_all(&scripts).unwrap();
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(scripts.join("bdc.exe"), b"").unwrap();
        fs::write(
            dist_info.join("METADATA"),
            "Name: bruce-doc-converter\nVersion: 0.1.2\n",
        )
        .unwrap();
        fs::write(dist_info.join("RECORD"), "../../Scripts/bdc.exe,,\n").unwrap();
        fs::write(
            dist_info.join("entry_points.txt"),
            "[console_scripts]\nbdc = bruce_doc_converter.cli:main\n",
        )
        .unwrap();

        assert_eq!(
            resolve_pip_package_name(&scripts.join("bdc.exe")),
            Some("bruce-doc-converter".to_string())
        );
    }
}
