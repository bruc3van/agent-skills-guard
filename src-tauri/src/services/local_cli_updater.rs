use crate::models::{LocalCliTool, PackageManager};
use crate::services::Database;
use anyhow::Result;
use chrono::Utc;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const CACHE_TTL_SECS: i64 = 3600;
const UPDATE_CHECK_CONCURRENCY: usize = 8;
const CLAUDE_RELEASES_BASE_URL: &str = "https://downloads.claude.ai/claude-code-releases";

fn supports_read_only_update_check(tool: &LocalCliTool) -> bool {
    tool.manager != PackageManager::Unknown
        && (tool.manager != PackageManager::Native || matches!(tool.id.as_str(), "grok" | "claude"))
}

pub(crate) fn is_cache_fresh(last_checked: Option<&str>) -> bool {
    let Some(ts) = last_checked else {
        return false;
    };
    let Ok(t) = ts.parse::<chrono::DateTime<Utc>>() else {
        return false;
    };
    Utc::now().signed_duration_since(t).num_seconds() < CACHE_TTL_SECS
}

pub(crate) fn is_outdated(current: Option<&str>, latest: Option<&str>) -> bool {
    match (current, latest) {
        (Some(c), Some(l)) => {
            let current = normalize_version(c);
            let latest = normalize_version(l);
            if current == latest {
                return false;
            }

            compare_version_like(&current, &latest)
                .map(|ordering| ordering == Ordering::Less)
                // An unknown version syntax is not evidence that an update exists.
                // This is deliberately conservative for PEP 440/Homebrew/vendor versions.
                .unwrap_or(false)
        }
        _ => false,
    }
}

fn normalize_version(v: &str) -> String {
    let v = v.trim();
    let v = v
        .strip_prefix('v')
        .or_else(|| v.strip_prefix('V'))
        .unwrap_or(v);
    v.to_string()
}

#[derive(Debug)]
struct VersionKey {
    parts: Vec<u64>,
    prerelease: Option<String>,
}

fn compare_version_like(current: &str, latest: &str) -> Option<Ordering> {
    let current = parse_version_key(current)?;
    let latest = parse_version_key(latest)?;

    let max_len = current.parts.len().max(latest.parts.len());
    for idx in 0..max_len {
        let current_part = current.parts.get(idx).copied().unwrap_or(0);
        let latest_part = latest.parts.get(idx).copied().unwrap_or(0);
        match current_part.cmp(&latest_part) {
            Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }

    Some(
        match (current.prerelease.as_deref(), latest.prerelease.as_deref()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(current_pre), Some(latest_pre)) => compare_prerelease(current_pre, latest_pre),
        },
    )
}

fn parse_version_key(version: &str) -> Option<VersionKey> {
    let without_build = version.split('+').next().unwrap_or(version);
    let (core, prerelease) = without_build
        .split_once('-')
        .map(|(core, prerelease)| (core, Some(prerelease.to_string())))
        .unwrap_or((without_build, None));

    let mut parts = Vec::new();
    for part in core.replace('_', ".").split('.') {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        parts.push(part.parse::<u64>().ok()?);
    }

    (!parts.is_empty()).then_some(VersionKey { parts, prerelease })
}

fn compare_prerelease(current: &str, latest: &str) -> Ordering {
    let current_parts = current.split('.').collect::<Vec<_>>();
    let latest_parts = latest.split('.').collect::<Vec<_>>();
    let max_len = current_parts.len().max(latest_parts.len());

    for idx in 0..max_len {
        match (current_parts.get(idx), latest_parts.get(idx)) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(current_part), Some(latest_part)) => {
                let current_num = current_part.parse::<u64>();
                let latest_num = latest_part.parse::<u64>();
                let ordering = match (current_num, latest_num) {
                    (Ok(current_num), Ok(latest_num)) => current_num.cmp(&latest_num),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => current_part.cmp(latest_part),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }

    Ordering::Equal
}

pub struct LocalCliUpdater {
    db: Arc<Database>,
}

impl LocalCliUpdater {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn check_updates(&self, tools: &mut Vec<LocalCliTool>) -> Result<()> {
        let semaphore = Arc::new(Semaphore::new(UPDATE_CHECK_CONCURRENCY));
        let mut tasks = Vec::new();
        let mut eligible = 0usize;
        let mut succeeded = 0usize;
        let mut failures = Vec::new();

        for tool in tools.iter_mut() {
            tool.update_check_error = None;
            if !supports_read_only_update_check(tool) {
                continue;
            }
            if is_cache_fresh(tool.last_checked.as_deref()) {
                eligible += 1;
                tool.update_available = is_outdated(
                    tool.current_version.as_deref(),
                    tool.latest_version.as_deref(),
                );
                let _ = self.db.upsert_local_cli_tool(
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
                succeeded += 1;
                continue;
            }

            let pkg_name = tool.effective_package_name().to_string();
            eligible += 1;
            let semaphore = Arc::clone(&semaphore);
            let task_tool = tool.clone();
            let detected_path = tool.detected_path.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.ok();
                let result = fetch_latest_for_tool(&task_tool, &pkg_name).await;
                (detected_path, task_tool.id, result)
            }));
        }

        for task in tasks {
            let (path, id, latest_result) = task.await?;
            let Some(tool) = tools.iter_mut().find(|t| t.detected_path == path) else {
                continue;
            };
            match latest_result {
                Ok(latest) => {
                    let latest = latest.strip_prefix('v').unwrap_or(&latest).to_string();
                    tool.update_available =
                        is_outdated(tool.current_version.as_deref(), Some(&latest));
                    tool.latest_version = Some(latest.clone());
                    tool.last_checked = Some(Utc::now().to_rfc3339());
                    let _ = self.db.upsert_local_cli_tool(
                        &tool.id,
                        &tool.detected_path,
                        tool.manager.as_str(),
                        tool.current_version.as_deref(),
                        Some(&latest),
                        tool.update_available,
                        tool.last_checked.as_deref(),
                        tool.package_name.as_deref(),
                        tool.description.as_deref(),
                    );
                    succeeded += 1;
                }
                Err(e) => {
                    let message = e.to_string();
                    tool.update_check_error = Some(message.clone());
                    failures.push(format!("{}: {}", id, message));
                    log::warn!("检查 {} 更新失败: {}", id, message);
                }
            }
        }
        if eligible > 0 && succeeded == 0 {
            anyhow::bail!(
                "所有 {} 个 CLI 更新检查均失败：{}",
                eligible,
                failures.join("；")
            );
        }
        Ok(())
    }
}

async fn fetch_latest_for_tool(tool: &LocalCliTool, pkg_name: &str) -> Result<String> {
    match tool.manager {
        PackageManager::Npm => fetch_node_latest_with_manager(tool, "npm", pkg_name).await,
        PackageManager::Pnpm => fetch_node_latest_with_manager(tool, "pnpm", pkg_name).await,
        PackageManager::Pip => fetch_pip_latest_with_manager(tool, pkg_name).await,
        PackageManager::Brew => fetch_brew_latest_with_manager(tool, pkg_name).await,
        PackageManager::Scoop => fetch_scoop_latest_with_manager(tool, pkg_name).await,
        PackageManager::Choco => fetch_choco_latest_with_manager(tool, pkg_name).await,
        PackageManager::Native if tool.id == "grok" => fetch_grok_latest(tool).await,
        PackageManager::Native if tool.id == "claude" => fetch_claude_latest().await,
        PackageManager::Native => {
            anyhow::bail!("native CLI does not support a read-only update check")
        }
        PackageManager::Unknown => anyhow::bail!("unknown package manager"),
    }
}

async fn fetch_pip_latest_with_manager(tool: &LocalCliTool, pkg_name: &str) -> Result<String> {
    let mut errors = Vec::new();
    for (executable, args) in pip_check_commands(tool, pkg_name) {
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        match run_command_output(&executable, &arg_refs, Duration::from_secs(20)).await {
            Ok(output) => match parse_pip_latest(&String::from_utf8(output.stdout)?, pkg_name) {
                Some(version) => return Ok(version),
                None => errors.push(format!(
                    "{} returned no latest version",
                    executable.to_string_lossy()
                )),
            },
            Err(error) => errors.push(format!("{}: {error}", executable.to_string_lossy())),
        }
    }

    anyhow::bail!("pip update check failed: {}", errors.join("; "))
}

fn parse_pip_latest(stdout: &str, pkg_name: &str) -> Option<String> {
    let prefix = format!("{} (", pkg_name.to_ascii_lowercase());
    stdout.lines().find_map(|line| {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        lower
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(')'))
            .map(ToOwned::to_owned)
    })
}

fn pip_check_commands(tool: &LocalCliTool, pkg_name: &str) -> Vec<(PathBuf, Vec<String>)> {
    let index_args = || {
        vec![
            "index".to_string(),
            "versions".to_string(),
            pkg_name.to_string(),
            "--disable-pip-version-check".to_string(),
        ]
    };
    let mut commands = Vec::new();
    if let Some(sibling) = sibling_manager_command(&tool.detected_path, "pip") {
        commands.push((sibling, index_args()));
    }

    // Keep the configured package source by invoking the user's own pip/Python process.
    commands.push((PathBuf::from("pip"), index_args()));
    let mut module_args = vec!["-m".to_string(), "pip".to_string()];
    module_args.extend(index_args());
    commands.push((PathBuf::from("python"), module_args.clone()));
    if cfg!(windows) {
        commands.push((PathBuf::from("py"), module_args));
    } else {
        commands.push((PathBuf::from("python3"), module_args));
    }
    commands
}

async fn fetch_brew_latest_with_manager(tool: &LocalCliTool, pkg_name: &str) -> Result<String> {
    let executable = manager_command_with_common_paths(
        tool,
        "brew",
        &["/opt/homebrew/bin/brew", "/usr/local/bin/brew"],
    );
    let output = run_command_output(
        &executable,
        &["info", "--json=v2", pkg_name],
        Duration::from_secs(20),
    )
    .await?;
    let body: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let formula = body["formulae"]
        .as_array()
        .and_then(|formulae| formulae.first())
        .ok_or_else(|| anyhow::anyhow!("brew info did not return formula metadata"))?;
    let stable = formula["versions"]["stable"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("brew info did not return a stable version"))?;
    let revision = formula["revision"].as_u64().unwrap_or(0);
    Ok(if revision > 0 {
        format!("{stable}_{revision}")
    } else {
        stable.to_string()
    })
}

async fn fetch_scoop_latest_with_manager(tool: &LocalCliTool, pkg_name: &str) -> Result<String> {
    let executable = manager_command_with_common_paths(tool, "scoop", &[]);
    let output =
        run_command_output(&executable, &["info", pkg_name], Duration::from_secs(20)).await?;
    parse_labeled_version(&String::from_utf8(output.stdout)?, "Version")
        .ok_or_else(|| anyhow::anyhow!("scoop info did not return a version"))
}

async fn fetch_choco_latest_with_manager(tool: &LocalCliTool, pkg_name: &str) -> Result<String> {
    let executable = manager_command_with_common_paths(tool, "choco", &[]);
    let output = run_command_output(
        &executable,
        &["search", pkg_name, "--exact", "--limit-output"],
        Duration::from_secs(20),
    )
    .await?;
    let stdout = String::from_utf8(output.stdout)?;
    stdout
        .lines()
        .find_map(|line| {
            line.trim()
                .split_once('|')
                .map(|(_, version)| version.trim().to_string())
        })
        .filter(|version| !version.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Chocolatey search did not return a version"))
}

fn parse_labeled_version(output: &str, label: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (key, value) = line.trim().split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(label)
            .then(|| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn manager_command_with_common_paths(
    tool: &LocalCliTool,
    manager: &str,
    common_paths: &[&str],
) -> PathBuf {
    sibling_manager_command(&tool.detected_path, manager)
        .or_else(|| {
            common_paths
                .iter()
                .map(PathBuf::from)
                .find(|path| path.is_file())
        })
        .unwrap_or_else(|| PathBuf::from(manager))
}

async fn fetch_node_latest_with_manager(
    tool: &LocalCliTool,
    manager: &str,
    pkg_name: &str,
) -> Result<String> {
    let executable = sibling_manager_command(&tool.detected_path, manager)
        .unwrap_or_else(|| PathBuf::from(manager));
    let output = run_command_output(
        &executable,
        &["view", pkg_name, "version", "--json"],
        Duration::from_secs(15),
    )
    .await?;
    let stdout = String::from_utf8(output.stdout)?.trim().to_string();
    if let Ok(version) = serde_json::from_str::<String>(&stdout) {
        return Ok(version);
    }
    let version = stdout.trim_matches('"').trim();
    if version.is_empty() {
        anyhow::bail!("{} registry response did not contain a version", manager);
    }
    Ok(version.to_string())
}

async fn fetch_grok_latest(tool: &LocalCliTool) -> Result<String> {
    let output = run_command_output(
        Path::new(&tool.detected_path),
        &["update", "--check", "--json"],
        Duration::from_secs(15),
    )
    .await?;
    let body: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    body["latestVersion"]
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("Grok update check did not return latestVersion"))
}

async fn fetch_claude_latest() -> Result<String> {
    let channel = claude_update_channel(dirs::home_dir().as_deref());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("agent-skills-guard/1.0")
        .build()?;
    let response = client
        .get(format!("{CLAUDE_RELEASES_BASE_URL}/{channel}"))
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!(
            "Claude release channel {} returned HTTP {}",
            channel,
            response.status()
        );
    }
    parse_claude_release_version(&response.text().await?)
        .ok_or_else(|| anyhow::anyhow!("Claude release channel returned an invalid version"))
}

fn claude_update_channel(home: Option<&Path>) -> &'static str {
    let Some(settings_path) = home.map(|home| home.join(".claude").join("settings.json")) else {
        return "latest";
    };
    let Ok(settings) = std::fs::read_to_string(settings_path) else {
        return "latest";
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&settings) else {
        return "latest";
    };
    match settings["autoUpdatesChannel"].as_str() {
        Some("stable") => "stable",
        _ => "latest",
    }
}

fn parse_claude_release_version(body: &str) -> Option<String> {
    let version = normalize_version(body);
    parse_version_key(&version).map(|_| version)
}

fn sibling_manager_command(detected_path: &str, manager: &str) -> Option<PathBuf> {
    let parent = Path::new(detected_path).parent()?;
    let names = if cfg!(windows) {
        vec![
            format!("{manager}.cmd"),
            format!("{manager}.exe"),
            manager.to_string(),
        ]
    } else {
        vec![manager.to_string()]
    };
    names
        .into_iter()
        .map(|name| parent.join(name))
        .find(|path| path.is_file())
}

async fn run_command_output(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output> {
    let extension = executable
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut command = if cfg!(windows) && matches!(extension.as_str(), "cmd" | "bat") {
        let mut command = tokio::process::Command::new("cmd.exe");
        command.arg("/d").arg("/c").arg(executable).args(args);
        command
    } else {
        let mut command = tokio::process::Command::new(executable);
        command.args(args);
        command
    };
    command.kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.as_std_mut().creation_flags(0x08000000);
    }
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| anyhow::anyhow!("command timed out after {}s", timeout.as_secs()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stderr.lines().take(3).collect::<Vec<_>>().join(" ");
        anyhow::bail!("command failed: {}", summary);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_fresh_within_one_hour() {
        let ts = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        assert!(is_cache_fresh(Some(&ts)));
    }

    #[test]
    fn cache_stale_after_one_hour() {
        let ts = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        assert!(!is_cache_fresh(Some(&ts)));
    }

    #[test]
    fn version_is_outdated_when_latest_differs() {
        assert!(is_outdated(Some("0.3.1"), Some("0.4.0")));
        assert!(!is_outdated(Some("0.4.0"), Some("0.4.0")));
        assert!(!is_outdated(None, Some("0.4.0")));
    }

    #[test]
    fn version_is_not_outdated_when_current_is_newer_than_latest() {
        assert!(!is_outdated(Some("11.14.1"), Some("11.12.1")));
        assert!(!is_outdated(Some("v11.14.1"), Some("11.12.1")));
        assert!(!is_outdated(Some("1.0.0"), Some("1.0.0-beta.1")));
        assert!(is_outdated(Some("11.12.1"), Some("11.14.1")));
        assert!(is_outdated(Some("1.0.0-beta.1"), Some("1.0.0")));
    }

    #[test]
    fn brew_revision_suffix_participates_in_comparison() {
        assert!(!is_outdated(Some("3.13.8_1"), Some("3.13.8")));
        assert!(is_outdated(Some("3.13.8"), Some("3.13.8_1")));
        assert!(!is_outdated(Some("3.13.8_2"), Some("3.13.8_1")));
        assert!(is_outdated(Some("3.13.7"), Some("3.13.8")));
        assert!(!is_outdated(Some("v3.13.8_1"), Some("3.13.8")));
    }

    #[test]
    fn incomparable_vendor_versions_do_not_create_false_updates() {
        assert!(!is_outdated(Some("2.0rc1"), Some("1.0rc1")));
        assert!(!is_outdated(Some("vendor-current"), Some("vendor-latest")));
    }

    #[test]
    fn pip_check_falls_back_from_path_to_python_module() {
        let tool = LocalCliTool::new("demo", "/home/user/.local/bin/demo", PackageManager::Pip);

        let commands = pip_check_commands(&tool, "demo-package");

        assert_eq!(commands[0].0, PathBuf::from("pip"));
        assert_eq!(commands[1].0, PathBuf::from("python"));
        assert_eq!(commands[1].1[..2], ["-m", "pip"]);
        assert!(commands[1].1.contains(&"demo-package".to_string()));
    }

    #[test]
    fn parses_pip_index_latest_version_case_insensitively() {
        assert_eq!(
            parse_pip_latest(
                "DEMO-PACKAGE (2.4.1)\nAvailable versions: 2.4.1",
                "demo-package"
            )
            .as_deref(),
            Some("2.4.1")
        );
    }

    #[test]
    fn claude_release_version_requires_a_valid_version() {
        assert_eq!(
            parse_claude_release_version("2.1.224\n").as_deref(),
            Some("2.1.224")
        );
        assert_eq!(parse_claude_release_version("<html>error</html>"), None);
    }

    #[test]
    fn claude_update_check_honors_the_configured_release_channel() {
        let dir = tempfile::tempdir().unwrap();
        let settings_dir = dir.path().join(".claude");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"autoUpdatesChannel":"stable"}"#,
        )
        .unwrap();

        assert_eq!(claude_update_channel(Some(dir.path())), "stable");
        assert_eq!(claude_update_channel(None), "latest");
    }

    #[test]
    fn native_claude_and_grok_support_read_only_update_checks() {
        for id in ["claude", "grok"] {
            let tool = LocalCliTool::new(id, "/usr/local/bin/tool", PackageManager::Native);
            assert!(supports_read_only_update_check(&tool));
        }
        let codex = LocalCliTool::new("codex", "/usr/local/bin/codex", PackageManager::Native);
        assert!(!supports_read_only_update_check(&codex));
    }

    #[tokio::test]
    async fn fresh_cache_recomputes_update_flag_from_detected_current_version() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(dir.path().join("test.db")).unwrap());
        let updater = LocalCliUpdater::new(Arc::clone(&db));
        let mut tool =
            LocalCliTool::new("bdc", r"C:\Python314\Scripts\bdc.exe", PackageManager::Pip);
        tool.current_version = Some("0.1.2".to_string());
        tool.latest_version = Some("0.1.3".to_string());
        tool.update_available = false;
        tool.last_checked = Some(Utc::now().to_rfc3339());

        let mut tools = vec![tool];

        updater.check_updates(&mut tools).await.unwrap();

        assert!(tools[0].update_available);
    }

    #[tokio::test]
    async fn fresh_cache_keeps_tool_clean_when_current_is_newer_than_latest() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(dir.path().join("test.db")).unwrap());
        let updater = LocalCliUpdater::new(Arc::clone(&db));
        let mut tool = LocalCliTool::new("npm", "/opt/homebrew/lib/npm", PackageManager::Npm);
        tool.current_version = Some("11.14.1".to_string());
        tool.latest_version = Some("11.12.1".to_string());
        tool.update_available = true;
        tool.last_checked = Some(Utc::now().to_rfc3339());
        tool.package_name = Some("npm".to_string());

        let mut tools = vec![tool];

        updater.check_updates(&mut tools).await.unwrap();

        assert!(!tools[0].update_available);
    }
}
