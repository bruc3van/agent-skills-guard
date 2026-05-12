use crate::models::{detect_manager_from_path, LocalCliTool, PackageManager};
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
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

    let bins = scan_path_dirs_for_supported_executables(path_dirs);
    let brew_installed_on_request = brew_installed_on_request_formulae();
    filter_brew_executables_to_installed_on_request(bins, brew_installed_on_request.as_ref())
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

fn brew_installed_on_request_formulae() -> Option<HashSet<String>> {
    let brew = find_brew_command()?;
    brew_installed_on_request_formulae_with(|args| brew_stdout(&brew, args))
}

fn brew_installed_on_request_formulae_with<F>(mut run: F) -> Option<HashSet<String>>
where
    F: FnMut(&[&str]) -> Option<String>,
{
    run(&["list", "--formula", "--installed-on-request"])
        .and_then(|stdout| parse_brew_list_output(&stdout))
        .or_else(|| run(&["leaves"]).and_then(|stdout| parse_brew_list_output(&stdout)))
}

fn find_brew_command() -> Option<PathBuf> {
    which::which("brew").ok().or_else(|| {
        ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
    })
}

fn brew_stdout(brew: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(brew)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout).ok()
}

fn parse_brew_list_output(stdout: &str) -> Option<HashSet<String>> {
    Some(
        stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn parse_brew_formula_executable_paths(stdout: &str) -> Vec<PathBuf> {
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| is_executable(path) && is_likely_cli_binary(path))
        .collect()
}

fn is_likely_cli_binary(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized.contains("/bin/")
        || normalized.contains("/sbin/")
        || normalized.contains("/libexec/bin/")
        || normalized.contains("/libexec/gnubin/")
}

fn brew_executable_paths_for_formula_with<F>(
    formula: &str,
    run: &mut F,
    linked_names: Option<&HashSet<String>>,
) -> Vec<PathBuf>
where
    F: FnMut(&[&str]) -> Option<String>,
{
    let mut paths = Vec::new();
    if let Some(prefix) = run(&["--prefix", formula]) {
        let prefix = PathBuf::from(prefix.trim());
        for subdir in &["bin", "sbin", "libexec/bin", "libexec/gnubin"] {
            paths.extend(scan_dir_for_executables(&prefix.join(subdir)));
        }
    }

    if paths.is_empty() {
        let list_args = ["list", "--formula", formula];
        paths = run(&list_args)
            .map(|stdout| parse_brew_formula_executable_paths(&stdout))
            .unwrap_or_default();
    }

    dedupe_paths(&mut paths);

    // Only include binaries that Homebrew actually links into <prefix>/bin/.
    // This filters out internal test/development scripts (e.g. ffmpeg's 40+ test tools,
    // imagemagick's *-config helpers, pipx's bundled python wrappers).
    if let Some(allowed) = linked_names {
        paths.retain(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| allowed.contains(name))
        });
    }

    paths.sort();
    paths
}

fn brew_tools_from_formulae_with<F>(formulae: &HashSet<String>, mut run: F) -> Vec<LocalCliTool>
where
    F: FnMut(&[&str]) -> Option<String>,
{
    let brew_prefix = run(&["--prefix"])
        .map(|stdout| PathBuf::from(stdout.trim()));
    let linked_binaries = brew_prefix
        .as_ref()
        .map(|prefix| brew_linked_binary_names(prefix))
        .unwrap_or_default();

    let mut formulae = formulae.iter().cloned().collect::<Vec<_>>();
    formulae.sort();

    let mut tools = Vec::new();
    for formula in formulae {
        let version = {
            let args = ["list", "--versions", formula.as_str()];
            run(&args).and_then(|output| parse_brew_list_versions_output(&output))
        };
        let description = {
            let args = ["desc", "--formula", formula.as_str()];
            run(&args).and_then(|output| parse_brew_desc_output(&output))
        };

        let linked_names = linked_binaries.get(&formula);

        let mut seen_ids: HashSet<String> = HashSet::new();
        for path in brew_executable_paths_for_formula_with(&formula, &mut run, linked_names) {
            let id = tool_id_from_path(&path);
            if !seen_ids.insert(id.clone()) {
                continue;
            }
            let mut tool = LocalCliTool::new(&id, &path.to_string_lossy(), PackageManager::Brew);
            tool.package_name = Some(formula.clone());
            tool.current_version = version.clone();
            tool.description = description.clone();
            tools.push(tool);
        }
    }

    tools.sort_by(|a, b| a.id.cmp(&b.id));
    tools
}

fn filter_brew_executables_to_installed_on_request(
    paths: Vec<PathBuf>,
    installed_on_request: Option<&HashSet<String>>,
) -> Vec<PathBuf> {
    let Some(installed_on_request) = installed_on_request else {
        return paths;
    };

    paths
        .into_iter()
        .filter(|path| {
            if detect_manager_from_path(path) != PackageManager::Brew {
                return true;
            }

            brew_formula_name_from_path(path)
                .is_some_and(|formula| installed_on_request.contains(&formula))
        })
        .collect()
}

fn brew_formula_name_from_path(path: &Path) -> Option<String> {
    std::fs::canonicalize(path)
        .ok()
        .and_then(|p| brew_formula_name_from_cellar_path(&p))
        .or_else(|| brew_formula_name_from_cellar_path(path))
}

fn brew_formula_name_from_cellar_path(path: &Path) -> Option<String> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .split_once("/Cellar/")
        .and_then(|(_, rest)| rest.split('/').next())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn brew_linked_binary_names(brew_prefix: &Path) -> HashMap<String, HashSet<String>> {
    let bin_dir = brew_prefix.join("bin");
    let Ok(entries) = std::fs::read_dir(&bin_dir) else {
        return HashMap::new();
    };
    let mut result: HashMap<String, HashSet<String>> = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_symlink() {
            continue;
        }
        let Ok(real) = std::fs::canonicalize(&path) else {
            continue;
        };
        let Some(formula) = brew_formula_name_from_cellar_path(&real) else {
            continue;
        };
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            result
                .entry(formula)
                .or_default()
                .insert(name.to_string());
        }
    }
    result
}

fn executable_dedupe_key(path: &Path) -> String {
    let id = tool_id_from_path(path);
    if detect_manager_from_path(path) == PackageManager::Pip && is_pip_launcher_id(&id) {
        let parent = path
            .parent()
            .map(|p| {
                let s = p.to_string_lossy().replace('\\', "/");
                if cfg!(any(target_os = "windows", target_os = "macos")) {
                    s.to_lowercase()
                } else {
                    s
                }
            })
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

#[derive(Debug, Deserialize)]
struct PackageListEntry {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NpmListOutput {
    dependencies: Option<HashMap<String, PackageListEntry>>,
}

#[derive(Debug, Deserialize)]
struct PnpmListRoot {
    dependencies: Option<HashMap<String, PackageListEntry>>,
}

#[derive(Debug, Deserialize)]
struct NodePackageJson {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    bin: Option<NodeBinField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum NodeBinField {
    String(String),
    Object(HashMap<String, serde_json::Value>),
}

#[derive(Debug, Default)]
struct PipShowOutput {
    summary: Option<String>,
    location: Option<PathBuf>,
    files: Vec<String>,
}

fn run_command_stdout(command: &str, args: &[&str]) -> Option<String> {
    let output = spawn_command(command, args).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}


fn spawn_command(command: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    // On Windows, .cmd/.bat scripts cannot be executed directly via CreateProcess — they require
    // cmd.exe. Resolve the command via `which` (which respects PATHEXT) so npm.cmd, pnpm.cmd, etc.
    // are detected and wrapped appropriately.
    #[cfg(windows)]
    if let Ok(resolved) = which::which(command) {
        let ext = resolved
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext == "cmd" || ext == "bat" {
            return Command::new("cmd")
                .arg("/c")
                .arg(&resolved)
                .args(args)
                .output();
        }
        return Command::new(resolved).args(args).output();
    }
    Command::new(command).args(args).output()
}

fn command_global_node_modules(command: &str) -> Option<PathBuf> {
    run_command_stdout(command, &["root", "-g"]).map(|stdout| PathBuf::from(stdout.trim()))
}

fn shim_path(bin_dir: &Path, binary_name: &str) -> PathBuf {
    // npm/pnpm create .cmd shims on Windows; .ps1 variants exist but are not on PATH by default.
    #[cfg(windows)]
    {
        bin_dir.join(format!("{}.cmd", binary_name))
    }

    #[cfg(not(windows))]
    {
        bin_dir.join(binary_name)
    }
}

fn node_package_json_path(node_modules_root: &Path, package_name: &str) -> PathBuf {
    package_name
        .split('/')
        .fold(node_modules_root.to_path_buf(), |acc, part| acc.join(part))
        .join("package.json")
}

fn node_package_bin_names(package_name: &str, bin: &NodeBinField) -> Vec<String> {
    match bin {
        NodeBinField::String(_) => {
            package_name
                .rsplit('/')
                .next()
                .filter(|name| !name.is_empty())
                .map(|name| vec![name.to_string()])
                .unwrap_or_default()
        }
        NodeBinField::Object(entries) => entries.keys().cloned().collect(),
    }
}

fn read_node_package_json(node_modules_root: &Path, package_name: &str) -> Option<NodePackageJson> {
    let path = node_package_json_path(node_modules_root, package_name);
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn node_tools_from_dependencies(
    node_modules_root: &Path,
    bin_dir: &Path,
    dependencies: HashMap<String, PackageListEntry>,
    manager: PackageManager,
) -> Vec<LocalCliTool> {
    let mut tools = Vec::new();

    for (listed_name, listed_entry) in dependencies {
        let Some(package_json) = read_node_package_json(node_modules_root, &listed_name) else {
            continue;
        };
        let Some(bin_field) = package_json.bin.as_ref() else {
            continue;
        };

        let package_name = package_json.name.as_deref().unwrap_or(&listed_name);
        let version = package_json.version.or(listed_entry.version);
        let description = package_json.description;

        for bin_name in node_package_bin_names(package_name, bin_field) {
            let detected_path = shim_path(bin_dir, &bin_name);
            let mut tool = LocalCliTool::new(
                &tool_id_from_path(&detected_path),
                &detected_path.to_string_lossy(),
                manager.clone(),
            );
            tool.current_version = version.clone();
            tool.package_name = Some(package_name.to_string());
            tool.description = description.clone();
            tools.push(tool);
        }
    }

    tools.sort_by(|a, b| a.id.cmp(&b.id));
    tools
}

fn npm_tools_from_list_output(
    node_modules_root: &Path,
    bin_dir: &Path,
    output: &str,
) -> Vec<LocalCliTool> {
    let Ok(parsed) = serde_json::from_str::<NpmListOutput>(output) else {
        return vec![];
    };
    let Some(dependencies) = parsed.dependencies else {
        return vec![];
    };

    node_tools_from_dependencies(
        node_modules_root,
        bin_dir,
        dependencies,
        PackageManager::Npm,
    )
}

fn pnpm_tools_from_list_output(
    node_modules_root: &Path,
    bin_dir: &Path,
    output: &str,
) -> Vec<LocalCliTool> {
    let Ok(parsed) = serde_json::from_str::<Vec<PnpmListRoot>>(output) else {
        return vec![];
    };
    let Some(dependencies) = parsed.into_iter().next().and_then(|root| root.dependencies) else {
        return vec![];
    };

    node_tools_from_dependencies(
        node_modules_root,
        bin_dir,
        dependencies,
        PackageManager::Pnpm,
    )
}

fn parse_pip_show_output(output: &str) -> PipShowOutput {
    let mut parsed = PipShowOutput::default();
    let mut in_files = false;

    for line in output.lines() {
        if in_files {
            if line.starts_with(' ') || line.starts_with('\t') {
                let file = line.trim();
                if !file.is_empty() {
                    parsed.files.push(file.to_string());
                }
                continue;
            }
            in_files = false;
        }

        if line == "Files:" {
            in_files = true;
            continue;
        }

        if let Some(summary) = line.strip_prefix("Summary:") {
            let summary = summary.trim();
            if !summary.is_empty() {
                parsed.summary = Some(summary.to_string());
            }
        } else if let Some(location) = line.strip_prefix("Location:") {
            let location = location.trim();
            if !location.is_empty() {
                parsed.location = Some(PathBuf::from(location));
            }
        }
    }

    parsed
}

fn pip_script_roots_from_location(location: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let normalized = location.to_string_lossy().replace('\\', "/").to_lowercase();

    if normalized.ends_with("/site-packages") {
        if let Some(parent) = location.parent() {
            roots.push(parent.join("Scripts"));
            roots.push(parent.join("bin"));

            if parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case("Lib") || name.eq_ignore_ascii_case("lib")
                })
            {
                if let Some(env_root) = parent.parent() {
                    roots.push(env_root.join("Scripts"));
                    roots.push(env_root.join("bin"));
                }
            } else if parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_lowercase().starts_with("python"))
            {
                if let Some(lib_root) = parent.parent() {
                    if lib_root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.eq_ignore_ascii_case("lib"))
                    {
                        if let Some(env_root) = lib_root.parent() {
                            roots.push(env_root.join("bin"));
                            roots.push(env_root.join("Scripts"));
                        }
                    }
                }
            }
        }
    }

    roots
}

fn common_pip_script_roots(home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = home {
        roots.push(home.join(".local").join("bin"));
    }

    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        let python = appdata.join("Python");
        if let Ok(entries) = std::fs::read_dir(python) {
            for entry in entries.flatten() {
                roots.push(entry.path().join("Scripts"));
            }
        }
    }

    roots
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
        PackageManager::Brew => detect_brew_version(path),
        PackageManager::Scoop | PackageManager::Choco | PackageManager::Unknown => None,
    }
}

pub fn discover_local_cli_tools() -> Vec<LocalCliTool> {
    let mut tools = Vec::new();
    tools.extend(discover_npm_tools());
    tools.extend(discover_pnpm_tools());
    tools.extend(discover_pip_tools());
    tools.extend(discover_brew_tools());
    tools.extend(discover_scoop_choco_tools());
    tools
}

fn discover_npm_tools() -> Vec<LocalCliTool> {
    let Some(node_modules_root) = command_global_node_modules("npm") else {
        return vec![];
    };
    let Some(bin_dir) = node_modules_root.parent().map(Path::to_path_buf) else {
        return vec![];
    };
    let Some(output) = run_command_stdout("npm", &["ls", "-g", "--depth=0", "--json"]) else {
        return vec![];
    };

    npm_tools_from_list_output(&node_modules_root, &bin_dir, &output)
}

fn discover_pnpm_tools() -> Vec<LocalCliTool> {
    let Some(node_modules_root) = command_global_node_modules("pnpm") else {
        return vec![];
    };
    let Some(bin_dir) = node_modules_root.parent().map(Path::to_path_buf) else {
        return vec![];
    };
    let Some(output) = run_command_stdout("pnpm", &["ls", "-g", "--depth=0", "--json"]) else {
        return vec![];
    };

    pnpm_tools_from_list_output(&node_modules_root, &bin_dir, &output)
}

fn find_pip_site_packages() -> Vec<PathBuf> {
    let mut results: Vec<PathBuf> = Vec::new();

    // Primary: ask pip where it installed itself (works when pip is on PATH).
    if let Some(stdout) = run_command_stdout("pip", &["show", "pip"]) {
        let show = parse_pip_show_output(&stdout);
        if let Some(location) = show.location {
            if location.is_dir() && !results.contains(&location) {
                results.push(location);
            }
        }
    }

    // Fallback: scan known filesystem paths, handles GUI apps that don't inherit user PATH.
    fallback_pip_site_packages(&mut results);
    results
}

fn fallback_pip_site_packages(out: &mut Vec<PathBuf>) {
    #[cfg(windows)]
    {
        // Python.org user installer: %LOCALAPPDATA%\Programs\Python\Python3XX\Lib\site-packages
        if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            collect_python_site_packages_under(
                &local_appdata.join("Programs").join("Python"),
                &["Lib", "site-packages"],
                out,
            );
        }
        // pip install --user packages: %APPDATA%\Python\Python3XX\site-packages
        if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
            collect_python_site_packages_under(
                &appdata.join("Python"),
                &["site-packages"],
                out,
            );
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(home) = dirs::home_dir() {
            collect_python_site_packages_under(
                &home.join(".local").join("lib"),
                &["site-packages"],
                out,
            );
        }
        for prefix in &["/usr/local/lib", "/usr/lib"] {
            collect_python_site_packages_under(&PathBuf::from(prefix), &["site-packages"], out);
        }
    }
}

fn collect_python_site_packages_under(parent: &Path, sub_path: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("python") {
            let mut path = entry.path();
            for sub in sub_path {
                path = path.join(sub);
            }
            if path.is_dir() && !out.contains(&path) {
                out.push(path);
            }
        }
    }
}

fn console_script_names(entry_points_content: &str) -> Vec<String> {
    let mut in_console_scripts = false;
    let mut names = Vec::new();
    for line in entry_points_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_console_scripts = trimmed == "[console_scripts]";
            continue;
        }
        if in_console_scripts && !trimmed.is_empty() && !trimmed.starts_with('#') {
            if let Some(name) = trimmed.split('=').next() {
                let name = name.trim().to_string();
                if !name.is_empty() {
                    names.push(name);
                }
            }
        }
    }
    names
}

fn find_script_in_roots(roots: &[PathBuf], script_name: &str) -> Option<PathBuf> {
    for root in roots {
        // On Windows, pip installs .exe launchers; .cmd shims may also exist.
        #[cfg(windows)]
        for candidate in &[
            root.join(format!("{}.exe", script_name)),
            root.join(format!("{}.cmd", script_name)),
            root.join(script_name),
        ] {
            if candidate.exists() {
                return Some(candidate.clone());
            }
        }

        #[cfg(not(windows))]
        {
            let candidate = root.join(script_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn pip_tools_from_dist_info(site_packages: &Path, script_roots: &[PathBuf]) -> Vec<LocalCliTool> {
    let Ok(entries) = std::fs::read_dir(site_packages) else {
        return vec![];
    };

    let mut tools = Vec::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    for entry in entries.flatten() {
        let dist_info_path = entry.path();
        let dir_name = entry.file_name().to_string_lossy().to_lowercase();
        if !dir_name.ends_with(".dist-info") {
            continue;
        }

        let Ok(entry_points_content) =
            std::fs::read_to_string(dist_info_path.join("entry_points.txt"))
        else {
            continue;
        };

        let script_names = console_script_names(&entry_points_content);
        if script_names.is_empty() {
            continue;
        }

        let metadata_path = dist_info_path.join("METADATA");
        let version = read_metadata_field(&metadata_path, "Version");
        let package_name = read_metadata_field(&metadata_path, "Name")
            .or_else(|| pip_dist_name_from_dir(&dist_info_path));
        let summary = read_metadata_field(&metadata_path, "Summary");

        for script_name in script_names {
            let Some(path) = find_script_in_roots(script_roots, &script_name) else {
                continue;
            };
            if !seen_paths.insert(path.clone()) {
                continue;
            }
            let mut tool = LocalCliTool::new(
                &tool_id_from_path(&path),
                &path.to_string_lossy(),
                PackageManager::Pip,
            );
            tool.current_version = version.clone();
            tool.package_name = package_name.clone();
            tool.description = summary.clone();
            tools.push(tool);
        }
    }

    tools.sort_by(|a, b| a.id.cmp(&b.id));
    tools
}

fn discover_pip_tools() -> Vec<LocalCliTool> {
    let mut all_tools: Vec<LocalCliTool> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    let site_packages_dirs = find_pip_site_packages();
    let leaf_names = pip_leaf_package_names(&site_packages_dirs);

    for site_packages in &site_packages_dirs {
        let mut script_roots = pip_script_roots_from_location(site_packages);
        script_roots.extend(common_pip_script_roots(dirs::home_dir()));
        dedupe_paths(&mut script_roots);

        for tool in pip_tools_from_dist_info(site_packages, &script_roots) {
            if !is_pip_tool_visible(&tool, &leaf_names) {
                continue;
            }
            if seen_ids.insert(tool.id.clone()) {
                all_tools.push(tool);
            }
        }
    }

    all_tools.sort_by(|a, b| a.id.cmp(&b.id));
    all_tools
}

fn is_pip_tool_visible(tool: &LocalCliTool, leaf_names: &HashSet<String>) -> bool {
    let Some(ref pkg_name) = tool.package_name else {
        return true;
    };
    if leaf_names.contains(&pkg_name.to_lowercase()) {
        return true;
    }
    // Always show when the tool id matches the package name (e.g. pip package → pip tool).
    // These are typically primary CLI tools even if depended on by other packages.
    tool.id == pkg_name.to_lowercase()
}

fn discover_brew_tools() -> Vec<LocalCliTool> {
    let Some(brew) = find_brew_command() else {
        return vec![];
    };
    let Some(installed_on_request) =
        brew_installed_on_request_formulae_with(|args| brew_stdout(&brew, args))
    else {
        return vec![];
    };

    brew_tools_from_formulae_with(&installed_on_request, |args| brew_stdout(&brew, args))
}

fn discover_scoop_choco_tools() -> Vec<LocalCliTool> {
    supported_path_executables()
        .into_iter()
        .filter_map(|path| {
            let manager = detect_manager_from_path(&path);
            matches!(manager, PackageManager::Scoop | PackageManager::Choco)
                .then(|| tool_from_path(&path, manager))
        })
        .collect()
}

fn supported_path_executables() -> Vec<PathBuf> {
    let mut path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    path_dirs.extend(common_cli_search_dirs(dirs::home_dir()));
    scan_path_dirs_for_supported_executables(path_dirs)
}

fn tool_from_path(path: &Path, manager: PackageManager) -> LocalCliTool {
    let id = tool_id_from_path(path);
    let package_name = resolve_package_name(path, &manager);
    let description = resolve_description(path, &manager);
    let mut tool = LocalCliTool::new(&id, &path.to_string_lossy(), manager);
    tool.current_version = detect_version(path);
    tool.package_name = package_name;
    tool.description = description;
    tool
}

// Deduplicates script-root search directories; distinct from dedupe_supported_executables which deduplicates tools by identity.
fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
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
        PackageManager::Brew => resolve_brew_description(path),
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
    let pkg_json_path = node_package_json_path(&npm_global.join("node_modules"), &package_name);
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
    let pkg_json_path = node_package_json_path(&npm_global.join("node_modules"), &id);
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

fn read_package_json_field(path: &Path, field: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    json[field].as_str().map(|value| value.to_string())
}

fn detect_npm_version(path: &Path) -> Option<String> {
    let npm_global = npm_global_root(path)?;
    let package_name = resolve_npm_package_name(path)?;
    read_package_json_field(
        &node_package_json_path(&npm_global.join("node_modules"), &package_name),
        "version",
    )
}

fn detect_brew_version(path: &Path) -> Option<String> {
    let formula = brew_formula_name_from_path(path)?;
    let brew = find_brew_command()?;
    let output = brew_stdout(&brew, &["list", "--versions", &formula])?;
    parse_brew_list_versions_output(&output)
}

fn resolve_brew_description(path: &Path) -> Option<String> {
    let formula = brew_formula_name_from_path(path)?;
    let brew = find_brew_command()?;
    let output = brew_stdout(&brew, &["desc", "--formula", &formula])?;
    parse_brew_desc_output(&output)
}

fn parse_brew_list_versions_output(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .skip(1)
        .last()
        .map(ToOwned::to_owned)
        .filter(|version| !version.is_empty())
}

fn parse_brew_desc_output(output: &str) -> Option<String> {
    output
        .trim()
        .split_once(':')
        .map(|(_, desc)| desc.trim().to_string())
        .filter(|desc| !desc.is_empty())
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
    PNPM_SHIM_PACKAGE_RE
        .captures(content)
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
    read_metadata_field_from_str(&meta, field)
}

fn read_metadata_field_from_str(meta: &str, field: &str) -> Option<String> {
    let prefix = format!("{}: ", field);
    meta.lines().find_map(|line| {
        line.strip_prefix(&prefix).and_then(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        })
    })
}

fn pip_leaf_package_names(site_packages_dirs: &[PathBuf]) -> HashSet<String> {
    let mut required: HashSet<String> = HashSet::new();
    let mut all_names: HashSet<String> = HashSet::new();

    for sp in site_packages_dirs {
        let Ok(entries) = std::fs::read_dir(sp) else { continue; };
        for entry in entries.flatten() {
            let path = entry.path();
            let dir_name = entry.file_name().to_string_lossy().to_lowercase();
            if !dir_name.ends_with(".dist-info") && !dir_name.ends_with(".egg-info") {
                continue;
            }
            let Ok(meta) = std::fs::read_to_string(path.join("METADATA")) else { continue; };

            if let Some(name) = read_metadata_field_from_str(&meta, "Name") {
                all_names.insert(name.to_lowercase());
            }

            for line in meta.lines() {
                if let Some(dep) = line.strip_prefix("Requires-Dist: ") {
                    let dep_name = pip_dep_name(dep);
                    if !dep_name.is_empty() {
                        required.insert(dep_name);
                    }
                }
            }
        }
    }

    all_names.retain(|name| !required.contains(name));
    all_names
}

fn pip_dep_name(requires_dist: &str) -> String {
    requires_dist
        .split(|c: char| {
            c == ' ' || c == ';' || c == '(' || c == ')' || c == '>' || c == '<' || c == '='
                || c == '~' || c == '!'
        })
        .next()
        .unwrap_or("")
        .trim()
        .split('[')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
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

    fn write_executable(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn scan_dir_finds_executables() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("my-tool");
        write_executable(&bin, b"#!/bin/sh\necho hello");
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
    fn parse_brew_list_versions_output_reads_installed_version() {
        assert_eq!(
            parse_brew_list_versions_output("ripgrep 14.1.1\n"),
            Some("14.1.1".to_string())
        );
        assert_eq!(
            parse_brew_list_versions_output("python@3.13 3.13.7 3.13.8\n"),
            Some("3.13.8".to_string())
        );
        assert_eq!(parse_brew_list_versions_output("ripgrep\n"), None);
    }

    #[test]
    fn parse_brew_desc_output_reads_description() {
        assert_eq!(
            parse_brew_desc_output("ripgrep: Search tool like grep and The Silver Searcher\n"),
            Some("Search tool like grep and The Silver Searcher".to_string())
        );
        assert_eq!(parse_brew_desc_output("ripgrep:"), None);
        assert_eq!(parse_brew_desc_output(""), None);
    }

    #[test]
    fn npm_list_output_discovers_top_level_packages_and_bins() {
        let dir = tempfile::tempdir().unwrap();
        let node_modules = dir.path().join("node_modules");
        let bin = dir.path().join("bin");
        let typescript = node_modules.join("typescript");
        let scoped = node_modules.join("@scope").join("pkg");
        fs::create_dir_all(&typescript).unwrap();
        fs::create_dir_all(&scoped).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::write(
            typescript.join("package.json"),
            r#"{"name":"typescript","version":"5.4.0","description":"TypeScript","bin":{"tsc":"./bin/tsc","tsserver":"./bin/tsserver"}}"#,
        )
        .unwrap();
        fs::write(
            scoped.join("package.json"),
            r#"{"name":"@scope/pkg","version":"1.0.0","description":"Scoped CLI","bin":"./cli.js"}"#,
        )
        .unwrap();

        let tools = npm_tools_from_list_output(
            &node_modules,
            &bin,
            r#"{"dependencies":{"typescript":{"version":"5.4.0"},"@scope/pkg":{"version":"1.0.0"}}}"#,
        );

        assert_eq!(tools.len(), 3);
        assert!(tools.iter().any(|tool| {
            tool.id == "tsc"
                && tool.manager == PackageManager::Npm
                && tool.package_name.as_deref() == Some("typescript")
                && tool.current_version.as_deref() == Some("5.4.0")
                && tool.description.as_deref() == Some("TypeScript")
                && tool.detected_path == shim_path(&bin, "tsc").to_string_lossy().as_ref()
        }));
        assert!(tools.iter().any(|tool| {
            tool.id == "pkg"
                && tool.package_name.as_deref() == Some("@scope/pkg")
                && tool.current_version.as_deref() == Some("1.0.0")
                && tool.description.as_deref() == Some("Scoped CLI")
                && tool.detected_path == shim_path(&bin, "pkg").to_string_lossy().as_ref()
        }));
    }

    #[test]
    fn npm_list_output_returns_empty_for_invalid_json() {
        let dir = tempfile::tempdir().unwrap();

        let tools = npm_tools_from_list_output(dir.path(), dir.path(), "{not-json");

        assert!(tools.is_empty());
    }

    #[test]
    fn pnpm_list_output_uses_array_shape() {
        let dir = tempfile::tempdir().unwrap();
        let node_modules = dir.path().join("node_modules");
        let bin = dir.path().join("bin");
        let package = node_modules.join("@mermaid-js").join("mermaid-cli");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::write(
            package.join("package.json"),
            r#"{"name":"@mermaid-js/mermaid-cli","version":"11.0.0","description":"Mermaid CLI","bin":{"mmdc":"./src/cli.js"}}"#,
        )
        .unwrap();

        let tools = pnpm_tools_from_list_output(
            &node_modules,
            &bin,
            r#"[{"name":"global","version":"0.0.0","dependencies":{"@mermaid-js/mermaid-cli":{"version":"11.0.0"}}}]"#,
        );

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "mmdc");
        assert_eq!(tools[0].manager, PackageManager::Pnpm);
        assert_eq!(
            tools[0].package_name.as_deref(),
            Some("@mermaid-js/mermaid-cli")
        );
        assert_eq!(tools[0].current_version.as_deref(), Some("11.0.0"));
        assert_eq!(tools[0].description.as_deref(), Some("Mermaid CLI"));
    }

    #[test]
    fn pnpm_list_output_returns_empty_without_dependencies() {
        let dir = tempfile::tempdir().unwrap();

        assert!(pnpm_tools_from_list_output(dir.path(), dir.path(), "[]").is_empty());
        assert!(
            pnpm_tools_from_list_output(dir.path(), dir.path(), r#"[{"name":"global"}]"#)
                .is_empty()
        );
    }

    #[test]
    fn console_script_names_parses_entry_points_txt() {
        let content = "[console_scripts]\nbdc = bruce_doc_converter.cli:main\nmarkitdown = markitdown.__main__:main\n\n[other_section]\nsomething = else\n";
        let names = console_script_names(content);
        assert_eq!(names, vec!["bdc", "markitdown"]);
    }

    #[test]
    fn console_script_names_returns_empty_when_no_console_scripts_section() {
        let content = "[gui_scripts]\napp = myapp:main\n";
        assert!(console_script_names(content).is_empty());
    }

    #[test]
    fn pip_tools_from_dist_info_discovers_console_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let site_packages = dir.path().join("site-packages");
        let scripts = dir.path().join("Scripts");
        let dist_info = site_packages.join("bruce_doc_converter-0.3.1.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::create_dir_all(&scripts).unwrap();
        fs::write(
            dist_info.join("entry_points.txt"),
            "[console_scripts]\nbdc = bruce_doc_converter.cli:main\n",
        )
        .unwrap();
        fs::write(
            dist_info.join("METADATA"),
            "Name: bruce-doc-converter\nVersion: 0.3.1\nSummary: Document converter\n",
        )
        .unwrap();

        #[cfg(windows)]
        let script_path = {
            fs::write(scripts.join("bdc.exe"), b"").unwrap();
            scripts.join("bdc.exe")
        };
        #[cfg(not(windows))]
        let script_path = {
            write_executable(&scripts.join("bdc"), b"#!/bin/sh\n");
            scripts.join("bdc")
        };

        let tools = pip_tools_from_dist_info(&site_packages, &[scripts]);

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "bdc");
        assert_eq!(
            tools[0].package_name.as_deref(),
            Some("bruce-doc-converter")
        );
        assert_eq!(tools[0].current_version.as_deref(), Some("0.3.1"));
        assert_eq!(tools[0].description.as_deref(), Some("Document converter"));
        assert_eq!(
            tools[0].detected_path,
            script_path.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn pip_tools_from_dist_info_skips_packages_without_entry_points_file() {
        let dir = tempfile::tempdir().unwrap();
        let site_packages = dir.path().join("site-packages");
        let dist_info = site_packages.join("requests-2.28.0.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(
            dist_info.join("METADATA"),
            "Name: requests\nVersion: 2.28.0\n",
        )
        .unwrap();

        let tools = pip_tools_from_dist_info(&site_packages, &[]);
        assert!(tools.is_empty());
    }

    #[test]
    fn pip_tools_from_dist_info_skips_scripts_missing_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let site_packages = dir.path().join("site-packages");
        let dist_info = site_packages.join("ghost_tool-1.0.0.dist-info");
        fs::create_dir_all(&dist_info).unwrap();
        fs::write(
            dist_info.join("entry_points.txt"),
            "[console_scripts]\nghost = ghost_tool:main\n",
        )
        .unwrap();
        fs::write(
            dist_info.join("METADATA"),
            "Name: ghost-tool\nVersion: 1.0.0\n",
        )
        .unwrap();

        let tools = pip_tools_from_dist_info(&site_packages, &[dir.path().to_path_buf()]);
        assert!(tools.is_empty());
    }

    #[test]
    fn collect_python_site_packages_under_finds_python_version_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let python_dir = dir.path().join("Python");
        let py313_site = python_dir.join("Python313").join("Lib").join("site-packages");
        let py314_site = python_dir.join("Python314").join("Lib").join("site-packages");
        let other_dir = python_dir.join("other").join("Lib").join("site-packages");
        fs::create_dir_all(&py313_site).unwrap();
        fs::create_dir_all(&py314_site).unwrap();
        fs::create_dir_all(&other_dir).unwrap();

        let mut out = Vec::new();
        collect_python_site_packages_under(&python_dir, &["Lib", "site-packages"], &mut out);

        assert!(out.contains(&py313_site));
        assert!(out.contains(&py314_site));
        assert!(!out.contains(&other_dir));
    }

    #[test]
    fn brew_request_formulae_prefers_installed_on_request() {
        let calls = std::cell::RefCell::new(Vec::new());

        let formulae = brew_installed_on_request_formulae_with(|args| {
            calls
                .borrow_mut()
                .push(args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            if args == ["list", "--formula", "--installed-on-request"] {
                Some("ripgrep\npandoc\n".to_string())
            } else {
                panic!("brew leaves should not be called when installed-on-request succeeds");
            }
        });

        assert_eq!(
            formulae,
            Some(
                ["ripgrep".to_string(), "pandoc".to_string()]
                    .into_iter()
                    .collect()
            )
        );
        assert_eq!(
            calls.into_inner(),
            vec![vec![
                "list".to_string(),
                "--formula".to_string(),
                "--installed-on-request".to_string()
            ]]
        );
    }

    #[test]
    fn brew_request_formulae_falls_back_to_leaves() {
        let calls = std::cell::RefCell::new(Vec::new());

        let formulae = brew_installed_on_request_formulae_with(|args| {
            calls
                .borrow_mut()
                .push(args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>());
            if args == ["list", "--formula", "--installed-on-request"] {
                None
            } else if args == ["leaves"] {
                Some("node\n".to_string())
            } else {
                None
            }
        });

        assert_eq!(formulae, Some(["node".to_string()].into_iter().collect()));
        assert_eq!(calls.into_inner().len(), 2);
    }

    #[test]
    fn brew_tools_are_discovered_from_formula_file_lists() {
        let dir = tempfile::tempdir().unwrap();
        let homebrew = dir.path().join("opt").join("homebrew");
        let rg = homebrew
            .join("Cellar")
            .join("ripgrep")
            .join("14.1.1")
            .join("bin")
            .join(if cfg!(windows) { "rg.cmd" } else { "rg" });
        let fd = homebrew
            .join("Cellar")
            .join("fd")
            .join("10.0.0")
            .join("bin")
            .join(if cfg!(windows) { "fd.cmd" } else { "fd" });
        let dependency = homebrew
            .join("Cellar")
            .join("openssl@3")
            .join("3.5.4")
            .join("bin")
            .join(if cfg!(windows) {
                "openssl.cmd"
            } else {
                "openssl"
            });
        write_executable(&rg, b"#!/bin/sh\n");
        write_executable(&fd, b"#!/bin/sh\n");
        write_executable(&dependency, b"#!/bin/sh\n");
        let formulae = ["ripgrep".to_string(), "fd".to_string()]
            .into_iter()
            .collect::<HashSet<_>>();

        let tools = brew_tools_from_formulae_with(&formulae, |args| match args {
            ["list", "--formula", "ripgrep"] => Some(format!(
                "{}\n{}/README.md\n",
                rg.to_string_lossy(),
                rg.parent().unwrap().parent().unwrap().to_string_lossy()
            )),
            ["list", "--formula", "fd"] => Some(fd.to_string_lossy().to_string()),
            ["list", "--versions", "ripgrep"] => Some("ripgrep 14.1.1\n".to_string()),
            ["list", "--versions", "fd"] => Some("fd 10.0.0\n".to_string()),
            ["desc", "--formula", "ripgrep"] => Some("ripgrep: Search tool\n".to_string()),
            ["desc", "--formula", "fd"] => Some("fd: Find entries\n".to_string()),
            _ => None,
        });

        let ids = tools
            .iter()
            .map(|tool| tool.id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(ids, ["rg", "fd"].into_iter().collect());
        assert!(!tools
            .iter()
            .any(|tool| tool.detected_path == dependency.to_string_lossy().as_ref()));
        assert!(tools.iter().any(|tool| {
            tool.id == "rg"
                && tool.manager == PackageManager::Brew
                && tool.package_name.as_deref() == Some("ripgrep")
                && tool.current_version.as_deref() == Some("14.1.1")
                && tool.description.as_deref() == Some("Search tool")
                && tool.detected_path == rg.to_string_lossy().as_ref()
        }));
    }

    #[test]
    fn brew_tools_fall_back_to_prefix_bin_when_file_list_has_no_executables() {
        let dir = tempfile::tempdir().unwrap();
        let prefix = dir
            .path()
            .join("opt")
            .join("homebrew")
            .join("Cellar")
            .join("pandoc")
            .join("3.7.0.2");
        let pandoc = prefix.join("bin").join(if cfg!(windows) {
            "pandoc.cmd"
        } else {
            "pandoc"
        });
        write_executable(&pandoc, b"#!/bin/sh\n");
        let formulae = ["pandoc".to_string()].into_iter().collect::<HashSet<_>>();

        let tools = brew_tools_from_formulae_with(&formulae, |args| match args {
            ["list", "--formula", "pandoc"] => Some(format!(
                "{}/INSTALL_RECEIPT.json\n",
                prefix.to_string_lossy()
            )),
            ["--prefix", "pandoc"] => Some(prefix.to_string_lossy().to_string()),
            ["list", "--versions", "pandoc"] => Some("pandoc 3.7.0.2\n".to_string()),
            ["desc", "--formula", "pandoc"] => Some("pandoc: Markup converter\n".to_string()),
            _ => None,
        });

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "pandoc");
        assert_eq!(tools[0].detected_path, pandoc.to_string_lossy().as_ref());
        assert_eq!(tools[0].package_name.as_deref(), Some("pandoc"));
    }

    #[test]
    fn brew_metadata_parsers_match_homebrew_commands_used_by_scanner() {
        let version_output = "ffmpeg 8.1_1\n";
        let desc_output = "ffmpeg: Play, record, convert, and stream audio and video\n";

        assert_eq!(
            parse_brew_list_versions_output(version_output),
            Some("8.1_1".to_string())
        );
        assert_eq!(
            parse_brew_desc_output(desc_output),
            Some("Play, record, convert, and stream audio and video".to_string())
        );
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
        write_executable(&unsupported, b"");

        let supported_dir = tempfile::tempdir().unwrap();
        let supported_root = supported_dir
            .path()
            .join("AppData")
            .join("Roaming")
            .join("npm");
        fs::create_dir_all(&supported_root).unwrap();
        let supported = supported_root.join("bruce-doc-converter.cmd");
        write_executable(&supported, b"@echo off\r\necho 1.0.0\r\n");

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
            write_executable(&py314_scripts.join(name), b"");
        }
        for name in ["pip3.exe", "pip3.13.exe"] {
            write_executable(&py313_scripts.join(name), b"");
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
    fn filters_brew_executables_to_installed_on_request_formulae() {
        let paths = vec![
            PathBuf::from("/opt/homebrew/Cellar/ripgrep/14.1.1/bin/rg"),
            PathBuf::from("/opt/homebrew/Cellar/openssl@3/3.5.4/bin/openssl"),
            PathBuf::from("/Users/example/.local/bin/bdc"),
        ];
        let installed_on_request = ["ripgrep".to_string()].into_iter().collect();

        let found =
            filter_brew_executables_to_installed_on_request(paths, Some(&installed_on_request));

        assert_eq!(
            found,
            vec![
                PathBuf::from("/opt/homebrew/Cellar/ripgrep/14.1.1/bin/rg"),
                PathBuf::from("/Users/example/.local/bin/bdc"),
            ]
        );
    }

    #[test]
    fn filter_brew_executables_drops_unresolved_brew_paths_when_filter_available() {
        let paths = vec![
            PathBuf::from("/opt/homebrew/bin/not-a-cellar-link"),
            PathBuf::from("/Users/example/.local/bin/bdc"),
        ];
        let installed_on_request = ["ripgrep".to_string()].into_iter().collect();

        let found =
            filter_brew_executables_to_installed_on_request(paths, Some(&installed_on_request));

        assert_eq!(found, vec![PathBuf::from("/Users/example/.local/bin/bdc")]);
    }

    #[test]
    fn brew_formula_name_from_path_resolves_opt_homebrew_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let homebrew = dir.path().join("opt").join("homebrew");
        let bin = homebrew.join("bin");
        let cellar_bin = homebrew
            .join("Cellar")
            .join("ripgrep")
            .join("14.1.1")
            .join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&cellar_bin).unwrap();
        let target = cellar_bin.join("rg");
        write_executable(&target, b"#!/bin/sh\n");
        let link = bin.join("rg");

        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(windows)]
        {
            if let Err(err) = std::os::windows::fs::symlink_file(&target, &link) {
                if err.raw_os_error() == Some(1314) {
                    return;
                }
                panic!("failed to create symlink: {err}");
            }
        }

        assert_eq!(
            brew_formula_name_from_path(&link),
            Some("ripgrep".to_string())
        );
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

    #[test]
    fn pip_dep_name_extracts_package_from_requires_dist() {
        assert_eq!(pip_dep_name("numpy>=1.20"), "numpy");
        assert_eq!(
            pip_dep_name("requests [security] >=2.25"),
            "requests"
        );
        assert_eq!(pip_dep_name("packaging"), "packaging");
        assert_eq!(
            pip_dep_name("typing_extensions; python_version < '3.10'"),
            "typing_extensions"
        );
        assert_eq!(
            pip_dep_name("importlib_metadata>=3.6; python_version < '3.10'"),
            "importlib_metadata"
        );
        assert_eq!(pip_dep_name(""), "");
    }

    #[test]
    fn pip_leaf_package_names_finds_top_level_packages() {
        let dir = tempfile::tempdir().unwrap();
        let sp = dir.path().join("site-packages");
        fs::create_dir_all(&sp).unwrap();

        // torch depends on numpy
        let torch = sp.join("torch-2.0.0.dist-info");
        fs::create_dir_all(&torch).unwrap();
        fs::write(
            torch.join("METADATA"),
            "Name: torch\nVersion: 2.0.0\nRequires-Dist: numpy>=1.20\n",
        )
        .unwrap();

        // numpy depends on nothing
        let numpy = sp.join("numpy-1.24.0.dist-info");
        fs::create_dir_all(&numpy).unwrap();
        fs::write(
            numpy.join("METADATA"),
            "Name: numpy\nVersion: 1.24.0\n",
        )
        .unwrap();

        // pip depends on nothing (but often depended on by others — test isolation ensures it's a leaf here)
        let pip = sp.join("pip-23.0.0.dist-info");
        fs::create_dir_all(&pip).unwrap();
        fs::write(pip.join("METADATA"), "Name: pip\nVersion: 23.0.0\n").unwrap();

        let leaves = pip_leaf_package_names(&[sp]);

        // numpy is required by torch → not a leaf
        assert!(!leaves.contains("numpy"));
        // torch is not required by anyone → leaf
        assert!(leaves.contains("torch"));
        // pip is not required by anyone (in this isolated test) → leaf
        assert!(leaves.contains("pip"));
    }

    #[test]
    fn is_pip_tool_visible_shows_leaf_and_self_named_tools() {
        let leaf_names = ["twine".to_string(), "numpy".to_string()]
            .into_iter()
            .collect::<HashSet<_>>();

        let tool = |id: &str, pkg: &str| LocalCliTool {
            id: id.to_string(),
            package_name: Some(pkg.to_string()),
            ..LocalCliTool::new(id, "", crate::models::PackageManager::Pip)
        };

        // twine is a leaf → visible
        assert!(is_pip_tool_visible(&tool("twine", "twine"), &leaf_names));
        // numpy is a leaf → its f2py tool is visible
        assert!(is_pip_tool_visible(&tool("f2py", "numpy"), &leaf_names));
        // pip is NOT a leaf, but id "pip" == package name "pip" → visible
        assert!(is_pip_tool_visible(&tool("pip", "pip"), &leaf_names));
        // bruce-doc-converter is not a leaf and id "bdc" ≠ "bruce-doc-converter" → hidden
        assert!(!is_pip_tool_visible(
            &tool("bdc", "bruce-doc-converter"),
            &leaf_names
        ));
    }
}
