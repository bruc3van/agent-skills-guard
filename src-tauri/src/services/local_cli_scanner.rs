use crate::models::{detect_manager_from_path, LocalCliTool, PackageManager};
use regex::Regex;
use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

static PNPM_SHIM_PACKAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"node_modules[/\\](@[^/\\]+[/\\][^/\\]+|[^/\\]+)[/\\]"#)
        .expect("pnpm shim package regex should compile")
});

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
    let mut path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    path_dirs.extend(common_cli_search_dirs(dirs::home_dir()));

    scan_path_dirs_for_supported_executables(path_dirs)
}

fn common_cli_search_dirs(home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = home {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join("AppData").join("Local").join("pnpm"));
        dirs.push(home.join("AppData").join("Local").join("pnpm").join("bin"));
        dirs.push(home.join("AppData").join("Roaming").join("pnpm"));
        dirs.push(home.join("Library").join("pnpm").join("bin"));
        dirs.push(home.join(".local").join("share").join("pnpm").join("bin"));
        dirs.push(home.join(".pnpm-global").join("bin"));
    }
    dirs
}

pub fn scan_path_dirs_for_supported_executables(path_dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for dir in path_dirs {
        for bin in scan_dir_for_executables(&dir) {
            if !is_supported_cli_path(&bin) {
                continue;
            }
            candidates.push(bin);
        }
    }

    dedupe_supported_executables(candidates)
}

fn dedupe_supported_executables(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();

    let mut paths = paths;
    paths.sort_by_key(|path| executable_preference_rank(path));

    for path in paths {
        let key = executable_dedupe_key(&path);
        if seen_keys.insert(key) {
            result.push(path);
        }
    }

    result
}

fn executable_dedupe_key(path: &Path) -> String {
    let id = tool_id_from_path(path);
    if detect_manager_from_path(path) == PackageManager::Pip && is_pip_launcher_id(&id) {
        let parent = path
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase())
            .unwrap_or_default();
        return format!("pip:{}", parent);
    }

    id
}

fn executable_preference_rank(path: &Path) -> u8 {
    let id = tool_id_from_path(path);
    if detect_manager_from_path(path) != PackageManager::Pip || !is_pip_launcher_id(&id) {
        return 10;
    }

    if id == "pip" {
        0
    } else if id == "pip3" {
        1
    } else {
        2
    }
}

fn is_pip_launcher_id(id: &str) -> bool {
    id == "pip"
        || id == "pip3"
        || id
            .strip_prefix("pip3.")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()))
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
        PackageManager::Pnpm => detect_pnpm_version(path),
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
        PackageManager::Pnpm => resolve_pnpm_package_name(path),
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
        PackageManager::Pnpm => resolve_pnpm_description(path),
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

fn resolve_pnpm_description(path: &Path) -> Option<String> {
    let pkg_json_path = pnpm_package_json_path(path)?;
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

fn resolve_pnpm_package_name(path: &Path) -> Option<String> {
    let pkg_json_path = pnpm_package_json_path(path)?;
    read_package_json_field(&pkg_json_path, "name").or_else(|| Some(tool_id_from_path(path)))
}

fn detect_pnpm_version(path: &Path) -> Option<String> {
    let pkg_json_path = pnpm_package_json_path(path)?;
    read_package_json_field(&pkg_json_path, "version")
}

fn pnpm_package_json_path(path: &Path) -> Option<PathBuf> {
    let id = tool_id_from_path(path);

    if let Some(name) = extract_pnpm_package_from_shim(path) {
        for root in pnpm_global_node_modules_roots(path) {
            let pkg_json_path = node_package_json_path(&root, &name);
            if pkg_json_path.exists() {
                return Some(pkg_json_path);
            }
        }
    }

    for pkg_json_path in pnpm_global_package_json_paths(path) {
        let Some(name) = read_package_json_field(&pkg_json_path, "name") else {
            continue;
        };
        if name == id || name.rsplit('/').next() == Some(id.as_str()) {
            return Some(pkg_json_path);
        }
    }

    None
}

fn node_package_json_path(node_modules_root: &Path, package_name: &str) -> PathBuf {
    package_name
        .split('/')
        .fold(node_modules_root.to_path_buf(), |acc, part| acc.join(part))
        .join("package.json")
}

fn extract_pnpm_package_from_shim(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    PNPM_SHIM_PACKAGE_RE
        .captures_iter(&content)
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str().replace('\\', "/"))
        .filter(|name| !name.starts_with(".pnpm/"))
        .last()
}

fn pnpm_home_from_bin_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let parent_name = parent
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if parent_name.eq_ignore_ascii_case("bin") {
        return parent.parent().map(Path::to_path_buf);
    }

    if parent_name.eq_ignore_ascii_case("pnpm") || parent_name.eq_ignore_ascii_case(".pnpm-global")
    {
        return Some(parent.to_path_buf());
    }

    None
}

fn pnpm_global_node_modules_roots(path: &Path) -> Vec<PathBuf> {
    let Some(home) = pnpm_home_from_bin_path(path) else {
        return vec![];
    };

    let mut roots = vec![home.join("global").join("node_modules")];
    let global = home.join("global");
    if let Ok(entries) = std::fs::read_dir(global) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_pnpm_global_version_dir(&path) {
                roots.push(path.join("node_modules"));
            }
        }
    }
    roots
}

fn is_pnpm_global_version_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()))
}

fn pnpm_global_package_json_paths(path: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for root in pnpm_global_node_modules_roots(path) {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let package_dir = entry.path();
            let package_name = entry.file_name().to_string_lossy().to_string();
            if package_name.starts_with('@') {
                if let Ok(scoped_entries) = std::fs::read_dir(package_dir) {
                    for scoped_entry in scoped_entries.flatten() {
                        result.push(scoped_entry.path().join("package.json"));
                    }
                }
            } else {
                result.push(package_dir.join("package.json"));
            }
        }
    }
    result
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
    let re = Regex::new(r#"node_modules[/\\](@[^/\\]+[/\\][^/\\]+|[^/\\]+)[/\\]"#).ok()?;
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

    #[test]
    fn scan_path_dedupes_pip_aliases_per_python_environment() {
        let dir = tempfile::tempdir().unwrap();
        let py314_scripts = dir.path().join("Python314").join("Scripts");
        let py313_scripts = dir.path().join("Python313").join("Scripts");
        fs::create_dir_all(&py314_scripts).unwrap();
        fs::create_dir_all(&py313_scripts).unwrap();

        for name in ["pip.exe", "pip3.exe", "pip3.14.exe"] {
            fs::write(py314_scripts.join(name), b"").unwrap();
        }
        for name in ["pip3.exe", "pip3.13.exe"] {
            fs::write(py313_scripts.join(name), b"").unwrap();
        }

        let found = scan_path_dirs_for_supported_executables(vec![
            py314_scripts.clone(),
            py313_scripts.clone(),
        ]);
        let file_names = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(found.len(), 2);
        assert!(found.contains(&py314_scripts.join("pip.exe")));
        assert!(found.contains(&py313_scripts.join("pip3.exe")));
        assert!(!file_names.iter().any(|name| name == "pip3.13.exe"));
    }

    #[test]
    fn common_cli_search_dirs_include_macos_and_user_bins() {
        let home = PathBuf::from("/Users/example");
        let dirs = common_cli_search_dirs(Some(home.clone()));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
        assert!(dirs.contains(&home.join(".local").join("bin")));
        assert!(dirs.contains(&home.join("AppData").join("Local").join("pnpm")));
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
    fn detect_version_and_description_read_pnpm_global_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_home = dir.path().join("Library").join("pnpm");
        let bin = pnpm_home.join("bin");
        let package_root = pnpm_home
            .join("global")
            .join("5")
            .join("node_modules")
            .join("@scope")
            .join("my-tool");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            r#"{"name":"@scope/my-tool","version":"2.3.4","description":"A pnpm CLI"}"#,
        )
        .unwrap();
        let shim = bin.join("my-tool");
        fs::write(&shim, b"#!/bin/sh\n").unwrap();

        assert_eq!(detect_version(&shim), Some("2.3.4".to_string()));
        assert_eq!(
            resolve_description_for_path(&shim),
            Some("A pnpm CLI".to_string())
        );
    }

    #[test]
    fn detect_version_reads_pnpm_package_from_virtual_store_shim() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_home = dir.path().join("Library").join("pnpm");
        let bin = pnpm_home.join("bin");
        let node_modules = pnpm_home.join("global").join("5").join("node_modules");
        let package_root = node_modules.join("@scope").join("my-tool");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            r#"{"name":"@scope/my-tool","version":"3.4.5"}"#,
        )
        .unwrap();
        let shim = bin.join("my-tool");
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\nnode \"{}/.pnpm/@scope+my-tool@3.4.5/node_modules/@scope/my-tool/bin.js\"\n",
                node_modules.to_string_lossy()
            ),
        )
        .unwrap();

        assert_eq!(detect_version(&shim), Some("3.4.5".to_string()));
    }

    #[test]
    fn detect_version_reads_unscoped_pnpm_package_from_shim() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_home = dir.path().join("Library").join("pnpm");
        let bin = pnpm_home.join("bin");
        let node_modules = pnpm_home.join("global").join("5").join("node_modules");
        let package_root = node_modules.join("typescript");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            r#"{"name":"typescript","version":"5.9.3"}"#,
        )
        .unwrap();
        let shim = bin.join("tsserver");
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\nnode \"{}/.pnpm/typescript@5.9.3/node_modules/typescript/bin/tsserver\"\n",
                node_modules.to_string_lossy()
            ),
        )
        .unwrap();

        assert_eq!(
            resolve_pnpm_package_name(&shim),
            Some("typescript".to_string())
        );
        assert_eq!(detect_version(&shim), Some("5.9.3".to_string()));
    }

    #[test]
    fn pnpm_home_from_bin_path_handles_known_layouts() {
        let dir = tempfile::tempdir().unwrap();

        let macos_home = dir.path().join("Library").join("pnpm");
        assert_eq!(
            pnpm_home_from_bin_path(&macos_home.join("bin").join("mmdc")),
            Some(macos_home)
        );

        let global_home = dir.path().join(".pnpm-global");
        assert_eq!(
            pnpm_home_from_bin_path(&global_home.join("bin").join("mmdc")),
            Some(global_home)
        );

        let roaming_home = dir.path().join("AppData").join("Roaming").join("pnpm");
        assert_eq!(
            pnpm_home_from_bin_path(&roaming_home.join("mmdc.cmd")),
            Some(roaming_home)
        );
    }

    #[test]
    fn pnpm_global_node_modules_roots_only_include_numeric_global_versions() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_home = dir.path().join("Library").join("pnpm");
        let global = pnpm_home.join("global");
        fs::create_dir_all(global.join("5").join("node_modules")).unwrap();
        fs::create_dir_all(global.join("10").join("node_modules")).unwrap();
        fs::create_dir_all(global.join("latest").join("node_modules")).unwrap();
        fs::create_dir_all(global.join("5x").join("node_modules")).unwrap();
        fs::create_dir_all(pnpm_home.join("bin")).unwrap();

        let roots = pnpm_global_node_modules_roots(&pnpm_home.join("bin").join("mmdc"));

        assert!(roots.contains(&global.join("node_modules")));
        assert!(roots.contains(&global.join("5").join("node_modules")));
        assert!(roots.contains(&global.join("10").join("node_modules")));
        assert!(!roots.contains(&global.join("latest").join("node_modules")));
        assert!(!roots.contains(&global.join("5x").join("node_modules")));
    }

    #[test]
    fn detect_version_ignores_pnpm_packages_in_non_numeric_global_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_home = dir.path().join("Library").join("pnpm");
        let bin = pnpm_home.join("bin");
        let package_root = pnpm_home
            .join("global")
            .join("latest")
            .join("node_modules")
            .join("my-tool");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("package.json"),
            r#"{"name":"my-tool","version":"9.9.9"}"#,
        )
        .unwrap();
        let shim = bin.join("my-tool");
        fs::write(&shim, b"#!/bin/sh\n").unwrap();

        assert_eq!(detect_version(&shim), None);
    }

    #[test]
    fn detect_version_does_not_execute_pnpm_shim() {
        let dir = tempfile::tempdir().unwrap();
        let pnpm_home = dir.path().join("AppData").join("Local").join("pnpm");
        let bin = pnpm_home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let marker = dir.path().join("executed.txt");
        let shim = bin.join("suspicious.cmd");
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

    #[cfg(windows)]
    #[test]
    fn extract_npm_package_from_unscoped_shim_stops_at_package_name() {
        let content = r#"@IF EXIST "%~dp0\node.exe" (
  "%~dp0\node.exe" "%~dp0\node_modules\typescript\bin\tsserver" %*
) ELSE (
  node "%~dp0\node_modules\typescript\bin\tsserver" %*
)"#;

        assert_eq!(
            extract_npm_package_from_shim(content),
            Some("typescript".to_string())
        );
    }

    #[cfg(windows)]
    #[test]
    fn extract_npm_package_from_scoped_shim_includes_scope() {
        let content = r#"@IF EXIST "%~dp0\node.exe" (
  "%~dp0\node.exe" "%~dp0\node_modules\@anthropic-ai\claude-code\cli.js" %*
) ELSE (
  node "%~dp0\node_modules\@anthropic-ai\claude-code\cli.js" %*
)"#;

        assert_eq!(
            extract_npm_package_from_shim(content),
            Some("@anthropic-ai/claude-code".to_string())
        );
    }
}
