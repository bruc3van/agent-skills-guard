use crate::i18n::validate_locale;
use crate::models::{
    FeaturedMarketplace, FeaturedMarketplaceOwner, FeaturedMarketplacesConfig, LocalizedText,
    Plugin, Repository, SecurityLevel, SecurityReport, Skill,
};
use crate::security::{ScanOptions, SecurityScanner};
use crate::services::claude_cli::{ClaudeCli, ClaudeCommand};
use crate::services::{Database, GitHubService};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use which::which;

#[derive(Debug, Deserialize)]
struct MarketplaceManifest {
    name: String,
    #[allow(dead_code)]
    description: Option<String>,
    plugins: Vec<MarketplacePluginEntry>,
}

#[derive(Debug, Deserialize)]
struct MarketplacePluginEntry {
    name: String,
    description: Option<String>,
    version: Option<String>,
    source: String,
    author: Option<AuthorField>,
}

#[derive(Debug, Deserialize)]
struct PluginManifest {
    name: String,
    description: Option<String>,
    version: Option<String>,
    author: Option<AuthorField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AuthorField {
    Simple(String),
    Detailed {
        name: Option<String>,
        email: Option<String>,
    },
}

impl AuthorField {
    fn to_display(&self) -> Option<String> {
        match self {
            AuthorField::Simple(value) => Some(value.clone()),
            AuthorField::Detailed { name, email } => match (name, email) {
                (Some(name), Some(email)) => Some(format!("{} <{}>", name, email)),
                (Some(name), None) => Some(name.clone()),
                (None, Some(email)) => Some(email.clone()),
                (None, None) => None,
            },
        }
    }
}

#[derive(Debug)]
struct ResolvedPlugin {
    plugin: Plugin,
    source_path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct PluginInstallStatus {
    pub plugin_id: String,
    pub plugin_name: String,
    pub status: String,
    pub output: String,
}

#[derive(Debug, Serialize)]
pub struct PluginInstallResult {
    pub marketplace_name: String,
    pub marketplace_repo: String,
    pub marketplace_status: String,
    pub raw_log: String,
    pub plugin_statuses: Vec<PluginInstallStatus>,
}

#[derive(Debug, Serialize)]
pub struct PluginUninstallResult {
    pub plugin_id: String,
    pub plugin_name: String,
    pub success: bool,
    pub raw_log: String,
}

#[derive(Debug, Serialize)]
pub struct MarketplaceRemoveResult {
    pub marketplace_name: String,
    pub marketplace_repo: String,
    pub success: bool,
    pub removed_plugins_count: usize,
    pub raw_log: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ClaudeMarketplace {
    pub name: String,
    pub source: Option<String>,
    pub repo: Option<String>,
    pub repository_url: Option<String>,
    pub install_location: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PluginUpdateResult {
    pub plugin_id: String,
    pub plugin_name: String,
    pub status: String,
    pub raw_log: String,
}

#[derive(Debug, Serialize)]
pub struct MarketplaceUpdateResult {
    pub marketplace_name: String,
    pub success: bool,
    pub raw_log: String,
}

#[derive(Debug, Serialize)]
pub struct SkillPluginUpgradeCandidate {
    pub skill_id: String,
    pub skill_name: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub marketplace_name: String,
    pub marketplace_repo: Option<String>,
    pub marketplace_repository_url: Option<String>,
    pub marketplace_add_command: Option<String>,
    pub latest_version: Option<String>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeMarketplaceListEntry {
    name: String,
    #[allow(dead_code)]
    source: Option<String>,
    repo: Option<String>,
    #[serde(rename = "installLocation", alias = "install_location")]
    install_location: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ClaudeInstalledPluginEntry {
    id: String,
    version: Option<String>,
    scope: Option<String>,
    enabled: Option<bool>,
    #[serde(rename = "installPath", alias = "install_path")]
    install_path: Option<String>,
    #[serde(rename = "installedAt", alias = "installed_at")]
    installed_at: Option<String>,
    #[serde(rename = "lastUpdated", alias = "last_updated")]
    last_updated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeAvailablePluginEntry {
    #[serde(rename = "pluginId", alias = "plugin_id")]
    plugin_id: String,
    name: Option<String>,
    #[serde(
        rename = "marketplaceName",
        alias = "marketplace_name",
        alias = "marketplace"
    )]
    marketplace_name: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudePluginListWithAvailable {
    #[serde(default, alias = "installedPlugins")]
    installed: Vec<ClaudeInstalledPluginEntry>,
    #[serde(default, alias = "availablePlugins")]
    available: Vec<ClaudeAvailablePluginEntry>,
}

pub struct PluginManager {
    db: Arc<Database>,
    github: GitHubService,
    scanner: SecurityScanner,
}

impl PluginManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            github: GitHubService::new(),
            scanner: SecurityScanner::new(),
        }
    }

    pub fn scan_cached_repository_plugins(
        &self,
        cache_path: &Path,
        repo_url: &str,
    ) -> Result<Vec<Plugin>> {
        let repo_root = find_repo_root(cache_path)?;
        let manifest = match read_marketplace_manifest(&repo_root) {
            Ok(Some(manifest)) => manifest,
            Ok(None) => return Ok(Vec::new()),
            Err(e) => {
                log::warn!("读取 marketplace.json 失败: {}", e);
                return Ok(Vec::new());
            }
        };

        let existing_plugins = self.db.get_plugins().unwrap_or_default();
        let existing_map: HashMap<String, Plugin> = existing_plugins
            .into_iter()
            .map(|plugin| (plugin.id.clone(), plugin))
            .collect();

        let mut plugins = Vec::new();
        for entry in manifest.plugins {
            let source = normalize_source(&entry.source);
            let source_path = match resolve_source_path(&repo_root, &source) {
                Ok(path) => path,
                Err(e) => {
                    log::warn!("插件路径无效，跳过: {} ({})", entry.name, e);
                    continue;
                }
            };

            let plugin_manifest = read_plugin_manifest(&source_path).ok();
            let name = plugin_manifest
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| entry.name.clone());

            let mut plugin = Plugin::new(
                name,
                repo_url.to_string(),
                manifest.name.clone(),
                source.clone(),
            );

            plugin.description = plugin_manifest
                .as_ref()
                .and_then(|m| m.description.clone())
                .or(entry.description.clone());
            plugin.version = plugin_manifest
                .as_ref()
                .and_then(|m| m.version.clone())
                .or(entry.version.clone());
            plugin.author = plugin_manifest
                .as_ref()
                .and_then(|m| m.author.as_ref().and_then(|a| a.to_display()))
                .or(entry.author.as_ref().and_then(|a| a.to_display()));

            // 不再要求 plugin.json 存在，marketplace.json 里的 plugins 条目足够 Claude Code CLI 安装

            if let Some(existing) = existing_map.get(&plugin.id) {
                plugin.marketplace_add_command = existing.marketplace_add_command.clone();
                plugin.plugin_install_command = existing.plugin_install_command.clone();
                plugin.installed = existing.installed;
                plugin.installed_at = existing.installed_at;
                plugin.installed_version = existing.installed_version.clone();
                plugin.claude_id = existing.claude_id.clone().or(plugin.claude_id);
                plugin.discovery_source = existing
                    .discovery_source
                    .clone()
                    .or(plugin.discovery_source);
                plugin.claude_scope = existing.claude_scope.clone();
                plugin.claude_enabled = existing.claude_enabled;
                plugin.claude_install_path = existing.claude_install_path.clone();
                plugin.claude_last_updated = existing.claude_last_updated;
                plugin.security_score = existing.security_score;
                plugin.security_level = existing.security_level.clone();
                plugin.security_issues = existing.security_issues.clone();
                plugin.security_report = existing.security_report.clone();
                plugin.scanned_at = existing.scanned_at;
                plugin.staging_path = existing.staging_path.clone();
                plugin.install_log = existing.install_log.clone();
                // 清除旧的 "unsupported" 状态，保留其他有效状态（如 blocked, installed 等）
                let status = existing.install_status.clone();
                if status.as_deref() != Some("unsupported") {
                    plugin.install_status = status.or(plugin.install_status);
                }
            }

            plugins.push(plugin);
        }

        Ok(plugins)
    }

    /// 同步 Claude Code CLI 的本地安装状态（用于识别非本程序安装的 plugins/marketplaces）。
    ///
    /// 原则：不直接读写 Claude 的缓存目录，仅通过 CLI `list --json` 获取状态并落库。
    pub async fn sync_claude_installed_state(&self, claude_command: Option<String>) -> Result<()> {
        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            // 未安装 Claude CLI 时，跳过同步，保持 DB 原样
            log::debug!("未找到 Claude Code CLI: {}，跳过 plugins 同步", cli_command);
            return Ok(());
        }

        let claude_cli = ClaudeCli::new(cli_command);
        let commands = vec![
            ClaudeCommand {
                args: vec![
                    "plugin".to_string(),
                    "marketplace".to_string(),
                    "list".to_string(),
                    "--json".to_string(),
                ],
                timeout: Duration::from_secs(15),
            },
            ClaudeCommand {
                args: vec![
                    "plugin".to_string(),
                    "list".to_string(),
                    "--json".to_string(),
                ],
                timeout: Duration::from_secs(15),
            },
        ];

        let cli_result = claude_cli.run(&commands)?;
        let marketplace_output = cli_result
            .outputs
            .get(0)
            .map(|o| o.output.as_str())
            .unwrap_or_default();
        let plugins_output = cli_result
            .outputs
            .get(1)
            .map(|o| o.output.as_str())
            .unwrap_or_default();

        let marketplaces: Vec<ClaudeMarketplaceListEntry> =
            match parse_json_output(marketplace_output) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!(
                        "解析 `claude plugin marketplace list --json` 失败，尝试解析文本输出: {}",
                        e
                    );
                    parse_marketplace_list_text(marketplace_output)
                        .into_iter()
                        .map(|m| ClaudeMarketplaceListEntry {
                            name: m.name,
                            source: m.source,
                            repo: m.repo,
                            install_location: m.install_location,
                        })
                        .collect()
                }
            };
        let installed_plugins: Vec<ClaudeInstalledPluginEntry> = parse_json_output(plugins_output)
            .context("解析 `claude plugin list --json` 输出失败")?;

        let mut marketplace_repo_url_by_name: HashMap<String, String> = HashMap::new();
        for entry in marketplaces {
            if let Some(repo_url) = marketplace_repo_url(&entry) {
                marketplace_repo_url_by_name.insert(entry.name, repo_url);
            }
        }

        let existing_plugins = self.db.get_plugins().unwrap_or_default();
        let mut plugins_by_claude_id: HashMap<String, Plugin> = HashMap::new();
        for plugin in existing_plugins {
            if let Some(claude_id) = plugin.claude_id.clone() {
                plugins_by_claude_id.insert(claude_id, plugin);
            }
        }

        let mut installed_claude_ids: HashSet<String> = HashSet::new();

        for entry in installed_plugins {
            installed_claude_ids.insert(entry.id.clone());

            let (plugin_name, marketplace_name) = match parse_claude_plugin_id(&entry.id) {
                Some(v) => v,
                None => {
                    log::warn!("无法解析 Claude plugin id: {}", entry.id);
                    continue;
                }
            };

            let repository_url = marketplace_repo_url_by_name
                .get(&marketplace_name)
                .cloned()
                .unwrap_or_else(|| "local".to_string());

            let mut plugin = plugins_by_claude_id
                .get(&entry.id)
                .cloned()
                .unwrap_or_else(|| {
                    let mut p = Plugin::new(
                        plugin_name.clone(),
                        repository_url.clone(),
                        marketplace_name.clone(),
                        "external".to_string(),
                    );
                    p.discovery_source = Some("claude_cli".to_string());
                    p
                });

            plugin.claude_id = Some(entry.id.clone());
            plugin.installed = true;
            plugin.installed_version = entry.version.clone();
            plugin.claude_scope = entry.scope.clone();
            plugin.claude_enabled = entry.enabled;
            plugin.claude_install_path = entry.install_path.clone();
            plugin.claude_last_updated = parse_datetime(&entry.last_updated);
            if plugin.installed_at.is_none() {
                plugin.installed_at = parse_datetime(&entry.installed_at);
            }

            // 让 UI 可以展示 marketplace 归属（外部安装时 DB 里没有可用清单）
            if plugin.marketplace_name.is_empty() {
                plugin.marketplace_name = marketplace_name.clone();
            }
            if plugin.name.is_empty() {
                plugin.name = plugin_name.clone();
            }

            self.db.save_plugin(&plugin)?;
        }

        // 反向同步：DB 标记为 installed 但 CLI 已不存在 -> 标记为未安装
        let current_plugins = self.db.get_plugins().unwrap_or_default();
        for plugin in current_plugins {
            let claude_id = match plugin.claude_id.as_deref() {
                Some(v) => v,
                None => continue,
            };

            if plugin.installed && !installed_claude_ids.contains(claude_id) {
                let mut updated = plugin.clone();
                updated.installed = false;
                updated.installed_at = None;
                updated.installed_version = None;
                updated.claude_scope = None;
                updated.claude_enabled = None;
                updated.claude_install_path = None;
                updated.claude_last_updated = None;
                self.db.save_plugin(&updated)?;
            }
        }

        Ok(())
    }

    pub async fn get_claude_marketplaces(
        &self,
        claude_command: Option<String>,
    ) -> Result<Vec<ClaudeMarketplace>> {
        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            return Ok(Vec::new());
        }

        let claude_cli = ClaudeCli::new(cli_command);
        let commands = vec![ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "list".to_string(),
                "--json".to_string(),
            ],
            timeout: Duration::from_secs(15),
        }];

        let cli_result = claude_cli.run(&commands)?;
        let output = cli_result
            .outputs
            .first()
            .map(|o| o.output.as_str())
            .unwrap_or_default();
        let entries: Vec<ClaudeMarketplaceListEntry> = match parse_json_output(output) {
            Ok(v) => v,
            Err(e) => {
                log::warn!(
                    "解析 `claude plugin marketplace list --json` 失败，尝试解析文本输出: {}",
                    e
                );
                parse_marketplace_list_text(output)
                    .into_iter()
                    .map(|m| ClaudeMarketplaceListEntry {
                        name: m.name,
                        source: m.source,
                        repo: m.repo,
                        install_location: m.install_location,
                    })
                    .collect()
            }
        };

        Ok(entries
            .into_iter()
            .map(|e| ClaudeMarketplace {
                repository_url: marketplace_repo_url(&e),
                name: e.name,
                source: e.source,
                repo: e.repo,
                install_location: e.install_location,
            })
            .collect())
    }

    /// 检查已安装 plugins 的更新：返回 Vec<(plugin_db_id, latest_version)>
    pub async fn check_plugins_updates(
        &self,
        claude_command: Option<String>,
    ) -> Result<Vec<(String, String)>> {
        self.sync_claude_installed_state(claude_command.clone())
            .await?;

        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            return Ok(Vec::new());
        }

        let claude_cli = ClaudeCli::new(cli_command);
        let commands = vec![ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--available".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }];

        let cli_result = claude_cli.run(&commands)?;
        let output = cli_result
            .outputs
            .first()
            .map(|o| o.output.as_str())
            .unwrap_or_default();
        let payload = parse_claude_plugin_list_with_available(output)
            .context("解析 `claude plugin list --json --available` 输出失败")?;

        let mut available_versions: HashMap<String, String> = HashMap::new();
        for entry in payload.available {
            if let Some(version) = entry.version {
                if !version.trim().is_empty() {
                    available_versions.insert(entry.plugin_id, version);
                }
            }
        }

        let current_plugins = self.db.get_plugins().unwrap_or_default();
        let mut plugins_by_claude_id: HashMap<String, Plugin> = HashMap::new();
        for plugin in current_plugins {
            if let Some(claude_id) = plugin.claude_id.clone() {
                plugins_by_claude_id.insert(claude_id, plugin);
            }
        }

        let mut updates = Vec::new();
        for installed in payload.installed {
            let installed_version = installed.version.unwrap_or_default();
            let latest = match available_versions.get(&installed.id) {
                Some(v) => v,
                None => continue,
            };

            if latest.trim().is_empty() || installed_version.trim().is_empty() {
                continue;
            }

            if latest != &installed_version {
                if let Some(plugin) = plugins_by_claude_id.get(&installed.id) {
                    updates.push((plugin.id.clone(), latest.clone()));
                }
            }
        }

        Ok(updates)
    }

    /// 更新单个 plugin（调用 Claude Code CLI），并写回日志/状态
    pub async fn update_plugin(
        &self,
        plugin_id: &str,
        claude_command: Option<String>,
    ) -> Result<PluginUpdateResult> {
        let plugin = self
            .db
            .get_plugins()?
            .into_iter()
            .find(|p| p.id == plugin_id)
            .context("未找到该插件")?;

        if !plugin.installed {
            anyhow::bail!("该插件尚未安装");
        }

        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            anyhow::bail!("未找到 Claude Code CLI: {}", cli_command);
        }

        let scope = plugin
            .claude_scope
            .clone()
            .unwrap_or_else(|| "user".to_string());
        let plugin_spec = plugin
            .claude_id
            .clone()
            .unwrap_or_else(|| plugin.plugin_spec());

        let claude_cli = ClaudeCli::new(cli_command);
        let commands = vec![ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "update".to_string(),
                "--scope".to_string(),
                scope,
                plugin_spec,
            ],
            timeout: Duration::from_secs(180),
        }];

        let cli_result = claude_cli.run(&commands)?;
        let output = cli_result
            .outputs
            .first()
            .map(|o| o.output.clone())
            .unwrap_or_default();
        let status = parse_plugin_update_output(&output);

        // 写回日志与状态；并再次同步以获取最新 installed_version 等字段
        let mut updated = plugin.clone();
        updated.install_log = Some(cli_result.raw_log.clone());
        updated.install_status = Some(status.clone());
        self.db.save_plugin(&updated)?;

        self.sync_claude_installed_state(None).await.ok();

        Ok(PluginUpdateResult {
            plugin_id: updated.id,
            plugin_name: updated.name,
            status,
            raw_log: cli_result.raw_log,
        })
    }

    /// 检查 marketplace 更新：返回 Vec<(marketplace_name, latest_head_short_sha)>
    pub async fn check_marketplaces_updates(
        &self,
        claude_command: Option<String>,
    ) -> Result<Vec<(String, String)>> {
        if which("git").is_err() {
            return Ok(Vec::new());
        }

        let marketplaces = self.get_claude_marketplaces(claude_command).await?;
        let mut updates = Vec::new();

        for mp in marketplaces {
            let install_location = match mp.install_location.as_deref() {
                Some(v) if !v.trim().is_empty() => v,
                _ => continue,
            };

            let repo = match mp.repo.as_deref() {
                Some(v) if !v.trim().is_empty() => v.trim(),
                _ => continue,
            };

            // 仅对 GitHub marketplace 做 HEAD 对比（owner/repo）
            let remote_url = if repo.starts_with("http://") || repo.starts_with("https://") {
                repo.to_string()
            } else {
                format!("https://github.com/{}.git", repo)
            };

            let local_head = git_output(&["-C", install_location, "rev-parse", "HEAD"]).ok();
            let remote_head = git_output(&["ls-remote", &remote_url, "HEAD"]).ok();
            let (Some(local_head), Some(remote_head)) = (local_head, remote_head) else {
                continue;
            };

            let local_head = local_head.trim().to_string();
            let remote_head_full = remote_head
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            if local_head.is_empty() || remote_head_full.is_empty() {
                continue;
            }

            if remote_head_full != local_head {
                let short = remote_head_full.chars().take(12).collect::<String>();
                updates.push((mp.name, short));
            }
        }

        Ok(updates)
    }

    /// 更新单个 marketplace（调用 Claude Code CLI）
    pub async fn update_marketplace(
        &self,
        marketplace_name: &str,
        claude_command: Option<String>,
    ) -> Result<MarketplaceUpdateResult> {
        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            anyhow::bail!("未找到 Claude Code CLI: {}", cli_command);
        }

        let claude_cli = ClaudeCli::new(cli_command);
        let commands = vec![ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "update".to_string(),
                marketplace_name.to_string(),
            ],
            timeout: Duration::from_secs(60),
        }];

        let cli_result = claude_cli.run(&commands)?;
        let output = cli_result
            .outputs
            .first()
            .map(|o| o.output.clone())
            .unwrap_or_default();
        let success = parse_marketplace_update_output(&output);

        Ok(MarketplaceUpdateResult {
            marketplace_name: marketplace_name.to_string(),
            success,
            raw_log: cli_result.raw_log,
        })
    }

    /// 检测：已安装 skills 中，哪些“也存在 Claude Code Plugin 版本”，用于提示用户升级为 plugin 完整安装。
    ///
    /// 说明：此处仅做“提示候选”，不自动安装/不移除 skill；安装仍建议走本应用的安全扫描 + plugin 安装流程。
    pub async fn get_skill_plugin_upgrade_candidates(
        &self,
        claude_command: Option<String>,
    ) -> Result<Vec<SkillPluginUpgradeCandidate>> {
        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            return Ok(Vec::new());
        }

        // 拉取 marketplaces（用于给出 marketplace add 的 repo 参数）
        let marketplaces = self
            .get_claude_marketplaces(Some(cli_command.clone()))
            .await?;
        let mut marketplace_repo_by_name: HashMap<String, (Option<String>, Option<String>)> =
            HashMap::new();
        for mp in marketplaces {
            marketplace_repo_by_name.insert(
                mp.name.clone(),
                (mp.repo.clone(), mp.repository_url.clone()),
            );
        }

        // 从 DB 获取 marketplace_add_command（来源于 featured-marketplace.yaml）
        let mut marketplace_add_cmd_by_name: HashMap<String, String> = HashMap::new();
        if let Ok(db_plugins) = self.db.get_plugins() {
            for p in db_plugins {
                if let Some(cmd) = p.marketplace_add_command {
                    marketplace_add_cmd_by_name
                        .entry(p.marketplace_name.clone())
                        .or_insert(cmd);
                }
            }
        }

        // 拉取 installed plugins（用于过滤：已安装的不提示）
        let claude_cli = ClaudeCli::new(cli_command.clone());
        let installed_output = claude_cli.run(&[ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "list".to_string(),
                "--json".to_string(),
            ],
            timeout: Duration::from_secs(20),
        }])?;
        let installed_text = installed_output
            .outputs
            .first()
            .map(|o| o.output.as_str())
            .unwrap_or_default();
        let installed_plugins: Vec<ClaudeInstalledPluginEntry> = parse_json_output(installed_text)
            .context("解析 `claude plugin list --json` 输出失败")?;
        let installed_ids: HashSet<String> = installed_plugins.into_iter().map(|p| p.id).collect();

        // 拉取 available plugins（用于匹配 skill->plugin）
        let available_output = claude_cli.run(&[ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--available".to_string(),
            ],
            timeout: Duration::from_secs(30),
        }])?;
        let available_text = available_output
            .outputs
            .first()
            .map(|o| o.output.as_str())
            .unwrap_or_default();
        let payload = parse_claude_plugin_list_with_available(available_text)
            .context("解析 `claude plugin list --json --available` 输出失败")?;

        // 选择每个 name 的“最优 marketplace”作为推荐目标（优先官方 marketplace）
        let mut best_by_name: HashMap<String, ClaudeAvailablePluginEntry> = HashMap::new();
        for entry in payload.available {
            let plugin_name = entry
                .name
                .clone()
                .or_else(|| parse_claude_plugin_id(&entry.plugin_id).map(|v| v.0))
                .unwrap_or_default();
            if plugin_name.trim().is_empty() {
                continue;
            }

            let key = plugin_name.to_lowercase();
            match best_by_name.get(&key) {
                None => {
                    best_by_name.insert(key, entry);
                }
                Some(existing) => {
                    let existing_mp = existing.marketplace_name.clone().unwrap_or_default();
                    let candidate_mp = entry.marketplace_name.clone().unwrap_or_default();
                    let existing_is_official = existing_mp == "claude-plugins-official";
                    let candidate_is_official = candidate_mp == "claude-plugins-official";
                    if candidate_is_official && !existing_is_official {
                        best_by_name.insert(key, entry);
                    }
                }
            }
        }

        let skills: Vec<Skill> = self.db.get_skills().unwrap_or_default();
        let installed_skills = skills
            .into_iter()
            .filter(|s| s.installed)
            .collect::<Vec<_>>();

        let mut candidates = Vec::new();
        for skill in installed_skills {
            let skill_name = skill.name.trim().to_string();
            if skill_name.is_empty() {
                continue;
            }

            let key = skill_name.to_lowercase();
            let Some(best) = best_by_name.get(&key) else {
                continue;
            };

            let (plugin_name, marketplace_name) = match parse_claude_plugin_id(&best.plugin_id) {
                Some(v) => v,
                None => continue,
            };

            if installed_ids.contains(&best.plugin_id) {
                continue;
            }

            let (marketplace_repo, marketplace_repository_url) = marketplace_repo_by_name
                .get(&marketplace_name)
                .cloned()
                .unwrap_or((None, None));

            let marketplace_add_command =
                marketplace_add_cmd_by_name.get(&marketplace_name).cloned();

            candidates.push(SkillPluginUpgradeCandidate {
                skill_id: skill.id,
                skill_name: skill.name,
                plugin_id: best.plugin_id.clone(),
                plugin_name,
                marketplace_name,
                marketplace_repo,
                marketplace_repository_url,
                marketplace_add_command,
                latest_version: best.version.clone(),
                reason: "name_match".to_string(),
            });
        }

        Ok(candidates)
    }

    async fn load_installed_featured_marketplace_plugins(
        &self,
        config: &FeaturedMarketplacesConfig,
        claude_command: Option<String>,
    ) -> HashMap<String, Vec<Plugin>> {
        let marketplaces = match self.get_claude_marketplaces(claude_command).await {
            Ok(list) => list,
            Err(e) => {
                log::warn!("获取 Claude marketplaces 失败: {}", e);
                return HashMap::new();
            }
        };

        if marketplaces.is_empty() {
            return HashMap::new();
        }

        let mut featured_by_name: HashMap<String, &FeaturedMarketplace> = HashMap::new();
        for category in &config.marketplace {
            for marketplace in &category.marketplaces {
                featured_by_name.insert(marketplace.marketplace_name.clone(), marketplace);
            }
        }

        let mut result = HashMap::new();
        for marketplace in marketplaces {
            let Some(featured) = featured_by_name.get(&marketplace.name) else {
                continue;
            };

            let install_location = marketplace
                .install_location
                .clone()
                .or_else(|| default_marketplace_install_location(&marketplace.name));
            let Some(install_location) = install_location else {
                continue;
            };

            let repo_root = PathBuf::from(&install_location);
            if !repo_root.exists() {
                continue;
            }

            let manifest = match read_marketplace_manifest(&repo_root) {
                Ok(Some(manifest)) => manifest,
                Ok(None) => continue,
                Err(e) => {
                    log::warn!("读取 marketplace.json 失败: {}", e);
                    continue;
                }
            };

            if manifest.name != marketplace.name {
                log::warn!(
                    "marketplace.json 名称与 CLI 不一致，跳过自动同步: {} vs {}",
                    manifest.name,
                    marketplace.name
                );
                continue;
            }

            let repo_url = featured
                .repository_url
                .clone()
                .unwrap_or_else(|| format!("https://github.com/{}", featured.marketplace_repo));

            let resolved = match resolve_marketplace_plugins(&repo_root, &repo_url, false) {
                Ok(list) => list,
                Err(e) => {
                    log::warn!("解析 marketplace 插件失败: {}", e);
                    continue;
                }
            };

            let plugins = resolved
                .into_iter()
                .map(|entry| entry.plugin)
                .collect::<Vec<_>>();
            if !plugins.is_empty() {
                result.insert(marketplace.name.clone(), plugins);
            }
        }

        result
    }

    /// 同步精选 Marketplace 插件清单到数据库（仅用于展示/安装入口）
    pub async fn sync_featured_marketplaces(
        &self,
        config: &FeaturedMarketplacesConfig,
        locale: &str,
        claude_command: Option<String>,
        sync_installed_marketplaces: bool,
    ) -> Result<()> {
        let locale = validate_locale(locale);
        let existing_plugins = self.db.get_plugins().unwrap_or_default();
        let existing_map: HashMap<String, Plugin> = existing_plugins
            .iter()
            .cloned()
            .map(|plugin| (plugin.id.clone(), plugin))
            .collect();
        let installed_marketplace_plugins = if sync_installed_marketplaces {
            self.load_installed_featured_marketplace_plugins(config, claude_command)
                .await
        } else {
            HashMap::new()
        };

        let mut featured_ids: HashSet<String> = HashSet::new();

        for category in &config.marketplace {
            for marketplace in &category.marketplaces {
                let repo_url = marketplace.repository_url.clone().unwrap_or_else(|| {
                    format!("https://github.com/{}", marketplace.marketplace_repo)
                });
                let marketplace_name = marketplace.marketplace_name.clone();

                let plugins_to_sync: Vec<Plugin> = if let Some(manifest_plugins) =
                    installed_marketplace_plugins.get(&marketplace_name)
                {
                    let config_plugins_by_name: HashMap<
                        String,
                        &crate::models::FeaturedMarketplacePlugin,
                    > = marketplace
                        .plugins
                        .iter()
                        .map(|plugin| (plugin.name.to_lowercase(), plugin))
                        .collect();

                    manifest_plugins
                        .iter()
                        .cloned()
                        .map(|mut plugin| {
                            plugin.discovery_source = Some("featured_marketplace".to_string());
                            plugin.marketplace_add_command =
                                marketplace.marketplace_add_command.clone();

                            if let Some(config_entry) =
                                config_plugins_by_name.get(&plugin.name.to_lowercase())
                            {
                                if plugin.description.is_none() {
                                    plugin.description =
                                        Some(localized_text(&config_entry.description, &locale));
                                }
                                if plugin.version.is_none() {
                                    plugin.version = config_entry.version.clone();
                                }
                                if plugin.author.is_none() {
                                    plugin.author = config_entry
                                        .author
                                        .as_ref()
                                        .and_then(author_to_display)
                                        .or_else(|| {
                                            marketplace.owner.as_ref().and_then(author_to_display)
                                        });
                                }
                                if plugin.plugin_install_command.is_none() {
                                    plugin.plugin_install_command =
                                        config_entry.install_command.clone();
                                }
                            } else if plugin.author.is_none() {
                                plugin.author =
                                    marketplace.owner.as_ref().and_then(author_to_display);
                            }

                            plugin
                        })
                        .collect()
                } else {
                    marketplace
                        .plugins
                        .iter()
                        .map(|entry| {
                            let source = entry.source.clone().unwrap_or_else(|| ".".to_string());
                            let mut plugin = Plugin::new(
                                entry.name.clone(),
                                repo_url.clone(),
                                marketplace_name.clone(),
                                source,
                            );

                            plugin.discovery_source = Some("featured_marketplace".to_string());
                            plugin.description = Some(localized_text(&entry.description, &locale));
                            plugin.version = entry.version.clone();
                            plugin.author = entry
                                .author
                                .as_ref()
                                .and_then(author_to_display)
                                .or_else(|| marketplace.owner.as_ref().and_then(author_to_display));
                            plugin.marketplace_add_command =
                                marketplace.marketplace_add_command.clone();
                            plugin.plugin_install_command = entry.install_command.clone();

                            plugin
                        })
                        .collect()
                };

                for mut plugin in plugins_to_sync {
                    if let Some(existing) = existing_map.get(&plugin.id) {
                        if plugin.marketplace_add_command.is_none() {
                            plugin.marketplace_add_command =
                                existing.marketplace_add_command.clone();
                        }
                        if plugin.plugin_install_command.is_none() {
                            plugin.plugin_install_command = existing.plugin_install_command.clone();
                        }
                        plugin.installed = existing.installed;
                        plugin.installed_at = existing.installed_at;
                        plugin.installed_version = existing.installed_version.clone();
                        plugin.claude_id = existing.claude_id.clone().or(plugin.claude_id);
                        plugin.claude_scope = existing.claude_scope.clone();
                        plugin.claude_enabled = existing.claude_enabled;
                        plugin.claude_install_path = existing.claude_install_path.clone();
                        plugin.claude_last_updated = existing.claude_last_updated;
                        plugin.security_score = existing.security_score;
                        plugin.security_level = existing.security_level.clone();
                        plugin.security_issues = existing.security_issues.clone();
                        plugin.security_report = existing.security_report.clone();
                        plugin.scanned_at = existing.scanned_at;
                        plugin.staging_path = existing.staging_path.clone();
                        plugin.install_log = existing.install_log.clone();
                        plugin.install_status = match existing.install_status.as_deref() {
                            Some("unsupported") => None,
                            _ => existing.install_status.clone(),
                        };
                    }

                    self.db.save_plugin(&plugin)?;
                    featured_ids.insert(plugin.id.clone());
                }
            }
        }

        for existing in existing_plugins {
            if existing.discovery_source.as_deref() != Some("featured_marketplace") {
                continue;
            }
            if existing.installed {
                continue;
            }
            if featured_ids.contains(&existing.id) {
                continue;
            }
            let _ = self.db.delete_plugin(&existing.id);
        }

        Ok(())
    }

    pub async fn prepare_plugin_installation(
        &self,
        plugin_id: &str,
        locale: &str,
    ) -> Result<SecurityReport> {
        let plugin = self
            .db
            .get_plugins()?
            .into_iter()
            .find(|p| p.id == plugin_id)
            .context("未找到该插件")?;

        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == plugin.repository_url)
            .context("未找到对应的仓库记录")?
            .clone();

        let cache_path = if let Some(existing_cache_path) = &repo.cache_path {
            let cache_path_buf = PathBuf::from(existing_cache_path);
            if cache_path_buf.exists() {
                cache_path_buf
            } else {
                self.download_and_cache_repository(&repo.id, &plugin.repository_url)
                    .await?
            }
        } else {
            self.download_and_cache_repository(&repo.id, &plugin.repository_url)
                .await?
        };

        let repo_root = find_repo_root(&cache_path)?;
        let mut resolved_plugins = resolve_marketplace_plugins(
            &repo_root,
            &plugin.repository_url,
            false, // 不强制要求 plugin.json 存在
        )?;

        if resolved_plugins.is_empty() {
            anyhow::bail!("未发现可安装的插件");
        }

        let existing_plugins = self.db.get_plugins().unwrap_or_default();
        let existing_map: HashMap<String, Plugin> = existing_plugins
            .into_iter()
            .map(|plugin| (plugin.id.clone(), plugin))
            .collect();

        for resolved in &mut resolved_plugins {
            if let Some(existing) = existing_map.get(&resolved.plugin.id) {
                resolved.plugin.installed = existing.installed;
                resolved.plugin.installed_at = existing.installed_at;
                resolved.plugin.installed_version = existing.installed_version.clone();
                resolved.plugin.claude_id = existing
                    .claude_id
                    .clone()
                    .or(resolved.plugin.claude_id.clone());
                resolved.plugin.discovery_source = existing
                    .discovery_source
                    .clone()
                    .or(resolved.plugin.discovery_source.clone());
                resolved.plugin.claude_scope = existing.claude_scope.clone();
                resolved.plugin.claude_enabled = existing.claude_enabled;
                resolved.plugin.claude_install_path = existing.claude_install_path.clone();
                resolved.plugin.claude_last_updated = existing.claude_last_updated;
                resolved.plugin.install_log = existing.install_log.clone();
                resolved.plugin.install_status = existing.install_status.clone();
            }
        }

        let marketplace_name = resolved_plugins
            .first()
            .map(|p| p.plugin.marketplace_name.clone())
            .unwrap_or_else(|| plugin.marketplace_name.clone());

        let mut reports = Vec::new();
        for resolved in &resolved_plugins {
            let report = self.scanner.scan_directory_with_options(
                resolved.source_path.to_str().context("插件目录路径无效")?,
                &resolved.plugin.id,
                locale,
                ScanOptions { skip_readme: true },
                None,
            )?;
            reports.push((resolved.plugin.clone(), report));
        }

        let merged_report = merge_reports(&reports, &marketplace_name);

        let now = Utc::now();
        let blocked = merged_report.blocked;
        for (plugin_entry, report) in reports {
            let mut updated = plugin_entry.clone();
            updated.security_score = Some(report.score);
            updated.security_level = Some(report.level.as_str().to_string());
            updated.security_issues = Some(report.issues.clone());
            updated.security_report = Some(report.clone());
            updated.scanned_at = Some(now);
            updated.staging_path = Some(repo_root.to_string_lossy().to_string());
            if blocked && !updated.installed {
                updated.install_status = Some("blocked".to_string());
            }
            self.db.save_plugin(&updated)?;
        }

        if blocked {
            let mut error_msg =
                "安全检测发现严重威胁，已禁止安装。\n\n检测到以下高危操作：\n".to_string();
            for (idx, issue) in merged_report.hard_trigger_issues.iter().enumerate() {
                error_msg.push_str(&format!("{}. {}\n", idx + 1, issue));
            }
            error_msg.push_str("\n这些操作可能对您的系统造成严重危害，强烈建议不要安装此插件。");
            anyhow::bail!(error_msg);
        }

        Ok(merged_report)
    }

    pub async fn confirm_plugin_installation(
        &self,
        plugin_id: &str,
        claude_command: Option<String>,
    ) -> Result<PluginInstallResult> {
        let plugin = self
            .db
            .get_plugins()?
            .into_iter()
            .find(|p| p.id == plugin_id)
            .context("未找到该插件")?;

        let marketplace_repo = plugin
            .marketplace_add_command
            .as_deref()
            .and_then(extract_marketplace_repo_from_command)
            .or_else(|| {
                Repository::from_github_url(&plugin.repository_url)
                    .ok()
                    .map(|(owner, repo_name)| format!("{}/{}", owner, repo_name))
            })
            .context("无法解析 marketplace repo")?;
        let marketplace_name = plugin.marketplace_name.clone();

        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            let mut message = format!("未找到 Claude Code CLI: {}", cli_command);
            if which("codex").is_ok() {
                message.push_str("\n检测到 Codex，但该流程仅支持 Claude Code Plugin。");
            }
            if which("opencode").is_ok() {
                message.push_str("\n检测到 OpenCode，但该流程仅支持 Claude Code Plugin。");
            }
            anyhow::bail!(message);
        }
        let claude_cli = ClaudeCli::new(cli_command);

        // 构建命令：1. marketplace add，2. 只安装选中的单个 plugin
        let mut commands = Vec::new();
        let add_args = plugin
            .marketplace_add_command
            .as_deref()
            .and_then(parse_slash_command_args)
            .unwrap_or_else(|| {
                vec![
                    "plugin".to_string(),
                    "marketplace".to_string(),
                    "add".to_string(),
                    marketplace_repo.clone(),
                ]
            });

        commands.push(ClaudeCommand {
            args: add_args,
            timeout: Duration::from_secs(60),
        });

        let install_args = plugin
            .plugin_install_command
            .as_deref()
            .and_then(parse_slash_command_args)
            .unwrap_or_else(|| {
                vec![
                    "plugin".to_string(),
                    "install".to_string(),
                    plugin.plugin_spec(),
                ]
            });

        // 只安装选中的单个 plugin
        commands.push(ClaudeCommand {
            args: install_args,
            timeout: Duration::from_secs(180),
        });

        let cli_result = claude_cli.run(&commands)?;
        let mut outputs = cli_result.outputs.into_iter();

        let marketplace_output = outputs.next().map(|o| o.output).unwrap_or_default();

        let marketplace_outcome = parse_marketplace_add_output(&marketplace_output);
        let marketplace_status = if marketplace_outcome.success {
            if marketplace_outcome.already {
                "already_added"
            } else {
                "added"
            }
        } else {
            "failed"
        };

        let now = Utc::now();
        let mut plugin_statuses = Vec::new();

        // 只处理选中的单个 plugin
        let output = outputs.next().map(|o| o.output).unwrap_or_default();
        let outcome = parse_plugin_install_output(&output);
        let status = if outcome.success {
            if outcome.already {
                "already_installed"
            } else {
                "installed"
            }
        } else {
            "failed"
        };

        let mut updated = plugin.clone();
        updated.install_status = Some(status.to_string());
        updated.install_log = Some(cli_result.raw_log.clone());
        updated.staging_path = None;
        if outcome.success {
            updated.installed = true;
            updated.installed_at = Some(now);
        }
        self.db.save_plugin(&updated)?;

        plugin_statuses.push(PluginInstallStatus {
            plugin_id: updated.id,
            plugin_name: updated.name,
            status: status.to_string(),
            output,
        });

        Ok(PluginInstallResult {
            marketplace_name,
            marketplace_repo,
            marketplace_status: marketplace_status.to_string(),
            raw_log: cli_result.raw_log,
            plugin_statuses,
        })
    }

    pub fn cancel_plugin_installation(&self, plugin_id: &str) -> Result<()> {
        let plugin = self
            .db
            .get_plugins()?
            .into_iter()
            .find(|p| p.id == plugin_id)
            .context("未找到该插件")?;

        // 只清除选中的单个 plugin 的 staging_path
        let mut updated = plugin.clone();
        updated.staging_path = None;
        self.db.save_plugin(&updated)?;

        Ok(())
    }

    /// 卸载单个 plugin
    pub async fn uninstall_plugin(
        &self,
        plugin_id: &str,
        claude_command: Option<String>,
    ) -> Result<PluginUninstallResult> {
        let plugin = self
            .db
            .get_plugins()?
            .into_iter()
            .find(|p| p.id == plugin_id)
            .context("未找到该插件")?;

        if !plugin.installed {
            anyhow::bail!("该插件尚未安装");
        }

        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            anyhow::bail!("未找到 Claude Code CLI: {}", cli_command);
        }
        let claude_cli = ClaudeCli::new(cli_command);

        let commands = vec![ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "uninstall".to_string(),
                plugin.plugin_spec(),
            ],
            timeout: Duration::from_secs(60),
        }];

        let cli_result = claude_cli.run(&commands)?;
        let output = cli_result
            .outputs
            .first()
            .map(|o| o.output.clone())
            .unwrap_or_default();

        let outcome = parse_plugin_uninstall_output(&output);

        let mut updated = plugin.clone();
        if outcome.success {
            updated.installed = false;
            updated.installed_at = None;
            updated.install_status = Some("uninstalled".to_string());
        } else {
            updated.install_status = Some("uninstall_failed".to_string());
        }
        updated.install_log = Some(cli_result.raw_log.clone());
        self.db.save_plugin(&updated)?;

        Ok(PluginUninstallResult {
            plugin_id: updated.id,
            plugin_name: updated.name,
            success: outcome.success,
            raw_log: cli_result.raw_log,
        })
    }

    /// 移除整个 marketplace（会自动卸载该 marketplace 的所有 plugins）
    pub async fn remove_marketplace(
        &self,
        marketplace_name: &str,
        marketplace_repo: &str,
        claude_command: Option<String>,
    ) -> Result<MarketplaceRemoveResult> {
        let cli_command = claude_command.unwrap_or_else(|| "claude".to_string());
        if which(&cli_command).is_err() {
            anyhow::bail!("未找到 Claude Code CLI: {}", cli_command);
        }

        if let Err(e) = self
            .sync_claude_installed_state(Some(cli_command.clone()))
            .await
        {
            log::warn!("同步 Claude plugins 状态失败（移除 marketplace 时）: {}", e);
        }

        let all_plugins = self.db.get_plugins()?;
        let target_plugins: Vec<Plugin> = all_plugins
            .into_iter()
            .filter(|plugin| plugin.marketplace_name == marketplace_name)
            .collect();
        let installed_plugins: Vec<Plugin> = target_plugins
            .iter()
            .filter(|plugin| plugin.installed)
            .cloned()
            .collect();
        let mut uninstall_results: HashMap<String, bool> = HashMap::new();

        for plugin in &installed_plugins {
            match self
                .uninstall_plugin(&plugin.id, Some(cli_command.clone()))
                .await
            {
                Ok(result) => {
                    uninstall_results.insert(plugin.id.clone(), result.success);
                }
                Err(e) => {
                    log::warn!("卸载 marketplace 插件失败: {} ({})", plugin.name, e);
                    uninstall_results.insert(plugin.id.clone(), false);
                }
            }
        }

        let claude_cli = ClaudeCli::new(cli_command);

        let commands = vec![ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "marketplace".to_string(),
                "remove".to_string(),
                marketplace_name.to_string(),
            ],
            timeout: Duration::from_secs(60),
        }];

        let cli_result = claude_cli.run(&commands)?;
        let output = cli_result
            .outputs
            .first()
            .map(|o| o.output.clone())
            .unwrap_or_default();

        let outcome = parse_marketplace_remove_output(&output);

        // 移除成功后，删除该 marketplace 下的所有 plugin 记录
        let mut removed_count = 0;
        if outcome.success {
            for plugin in target_plugins {
                let uninstall_ok = uninstall_results.get(&plugin.id).copied().unwrap_or(true);
                if uninstall_ok {
                    self.db.delete_plugin(&plugin.id)?;
                    removed_count += 1;
                }
            }
        }

        Ok(MarketplaceRemoveResult {
            marketplace_name: marketplace_name.to_string(),
            marketplace_repo: marketplace_repo.to_string(),
            success: outcome.success && uninstall_results.values().all(|ok| *ok),
            removed_plugins_count: removed_count,
            raw_log: cli_result.raw_log,
        })
    }

    async fn download_and_cache_repository(
        &self,
        repo_id: &str,
        repo_url: &str,
    ) -> Result<PathBuf> {
        let (owner, repo_name) = Repository::from_github_url(repo_url)?;
        let cache_base_dir = dirs::cache_dir()
            .context("无法获取系统缓存目录")?
            .join("agent-skills-guard")
            .join("repositories");

        let (extract_dir, commit_sha) = self
            .github
            .download_repository_archive(&owner, &repo_name, &cache_base_dir)
            .await
            .context("下载仓库压缩包失败")?;

        let cache_path_str = extract_dir.to_string_lossy().to_string();
        self.db
            .update_repository_cache(repo_id, &cache_path_str, Utc::now(), Some(&commit_sha))
            .context("更新仓库缓存信息失败")?;

        Ok(extract_dir)
    }
}

fn parse_claude_plugin_id(id: &str) -> Option<(String, String)> {
    let (name, marketplace) = id.rsplit_once('@')?;
    if name.is_empty() || marketplace.is_empty() {
        return None;
    }
    Some((name.to_string(), marketplace.to_string()))
}

fn parse_slash_command_args(command: &str) -> Option<Vec<String>> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trimmed = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let parts: Vec<String> = trimmed.split_whitespace().map(|s| s.to_string()).collect();
    if parts.first().map(|s| s.as_str()) != Some("plugin") {
        return None;
    }
    Some(parts)
}

fn extract_marketplace_repo_from_command(command: &str) -> Option<String> {
    let parts = parse_slash_command_args(command)?;
    if parts.len() >= 4 && parts[0] == "plugin" && parts[1] == "marketplace" && parts[2] == "add" {
        return Some(parts[3].clone());
    }
    None
}

fn marketplace_repo_url(entry: &ClaudeMarketplaceListEntry) -> Option<String> {
    let repo = entry.repo.as_deref()?.trim();
    if repo.is_empty() {
        return None;
    }

    // Claude CLI 的 github source 通常返回 owner/repo
    if repo.starts_with("http://") || repo.starts_with("https://") {
        return Some(repo.to_string());
    }

    Some(format!("https://github.com/{}", repo))
}

fn parse_datetime(value: &Option<String>) -> Option<DateTime<Utc>> {
    value.as_ref().and_then(|s| s.parse().ok())
}

fn parse_claude_plugin_list_with_available(output: &str) -> Result<ClaudePluginListWithAvailable> {
    let cleaned = strip_terminal_escapes(output);

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&cleaned) {
        if json_has_plugin_list_fields(&value) {
            return serde_json::from_value(value).context("JSON 解析失败");
        }
    }

    if let Some(value) = find_json_value_with_predicate(&cleaned, json_has_plugin_list_fields) {
        return serde_json::from_value(value).context("JSON 解析失败");
    }

    parse_json_output(&cleaned).context("JSON 解析失败")
}

fn parse_json_output<T: for<'de> Deserialize<'de>>(output: &str) -> Result<T> {
    let cleaned = strip_terminal_escapes(output);
    // 1) 优先直接解析（输出本身就是纯 JSON 的情况）
    if let Ok(value) = serde_json::from_str::<T>(&cleaned) {
        return Ok(value);
    }

    // 2) 兼容：输出前后混有提示符/日志/ANSI 等，尝试提取一个完整 JSON 值并解析
    if let Ok(value) = parse_first_json_value::<T>(&cleaned) {
        return Ok(value);
    }

    // 3) 兜底：旧逻辑（按首尾括号截取），有助于处理一些更“干净但带前缀”的输出
    let payload = extract_json_payload(&cleaned).unwrap_or(cleaned.as_str());
    serde_json::from_str(payload).context("JSON 解析失败")
}

fn extract_json_payload(output: &str) -> Option<&str> {
    let start = output.find(|c| c == '{' || c == '[')?;
    let end = output.rfind(|c| c == '}' || c == ']')?;
    if end < start {
        return None;
    }
    Some(&output[start..=end])
}

fn parse_first_json_value<T: for<'de> Deserialize<'de>>(output: &str) -> Result<T> {
    // 通过 serde_json 的流式反序列化能力，从任意位置尝试解析出“第一个匹配的 JSON 值”
    // 这样可以兼容 PowerShell/Terminal 的提示符、以及 CLI 可能输出的非 JSON 文本或日志。
    let bytes = output.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        let offset = match bytes[pos..].iter().position(|b| *b == b'{' || *b == b'[') {
            Some(value) => value,
            None => break,
        };
        let start = pos + offset;
        let slice = &output[start..];
        let mut stream = serde_json::Deserializer::from_str(slice).into_iter::<serde_json::Value>();
        let value = match stream.next() {
            Some(Ok(v)) => v,
            _ => {
                pos = start + 1;
                continue;
            }
        };

        let end = stream.byte_offset();
        if end == 0 || end > slice.len() {
            pos = start + 1;
            continue;
        }

        // 只取 JSON 值本体，忽略后续的任何噪声输出
        let payload = &slice[..end];
        match serde_json::from_str::<T>(payload).or_else(|_| serde_json::from_value::<T>(value)) {
            Ok(parsed) => return Ok(parsed),
            Err(_) => {
                pos = start + end;
                continue;
            }
        }
    }

    anyhow::bail!("JSON 解析失败");
}

fn json_has_plugin_list_fields(value: &serde_json::Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };

    obj.contains_key("installed")
        || obj.contains_key("available")
        || obj.contains_key("installedPlugins")
        || obj.contains_key("availablePlugins")
}

fn find_json_value_with_predicate<F>(output: &str, predicate: F) -> Option<serde_json::Value>
where
    F: Fn(&serde_json::Value) -> bool,
{
    let bytes = output.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        let offset = match bytes[pos..].iter().position(|b| *b == b'{' || *b == b'[') {
            Some(value) => value,
            None => break,
        };
        let start = pos + offset;
        let slice = &output[start..];
        let mut stream = serde_json::Deserializer::from_str(slice).into_iter::<serde_json::Value>();
        let value = match stream.next() {
            Some(Ok(v)) => v,
            _ => {
                pos = start + 1;
                continue;
            }
        };

        let end = stream.byte_offset();
        if end == 0 || end > slice.len() {
            pos = start + 1;
            continue;
        }

        if predicate(&value) {
            return Some(value);
        }

        pos = start + end;
    }

    None
}

fn strip_terminal_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\u{001b}' {
            // CSI: ESC [
            if i + 1 < chars.len() && chars[i + 1] == '[' {
                i += 2;
                while i < chars.len() {
                    let c = chars[i];
                    // CSI sequences typically end with a byte in @..~
                    if ('@'..='~').contains(&c) {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // OSC: ESC ]
            if i + 1 < chars.len() && chars[i + 1] == ']' {
                i += 2;
                while i < chars.len() {
                    let c = chars[i];
                    // BEL ends OSC
                    if c == '\u{0007}' {
                        i += 1;
                        break;
                    }
                    // ST ends OSC: ESC \
                    if c == '\u{001b}' && i + 1 < chars.len() && chars[i + 1] == '\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Other ESC sequences: best-effort skip next char
            i += 1;
            if i < chars.len() {
                i += 1;
            }
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_claude_plugin_list_with_available_from_powershell_output() {
        let output = r#"PS C:\Users\Bruce> claude plugin list --json --available
{
  "installed": [
    {
      "id": "superpowers@superpowers-marketplace",
      "version": "4.0.3",
      "scope": "user",
      "enabled": false,
      "installPath": "C:\\Users\\Bruce\\.claude\\plugins\\cache\\superpowers-marketplace\\superpowers\\4.0.3",
      "installedAt": "2025-12-26T01:58:19.521Z",
      "lastUpdated": "2026-01-14T01:51:11.830Z"
    }
  ],
  "available": [
    {
      "pluginId": "superpowers@claude-plugins-official",
      "name": "superpowers",
      "marketplaceName": "claude-plugins-official",
      "version": "4.0.4",
      "source": {
        "source": "url",
        "url": "https://github.com/obra/superpowers.git"
      },
      "installCount": 123
    }
  ]
}
PS C:\Users\Bruce> "#;

        let payload: ClaudePluginListWithAvailable = parse_json_output(output).unwrap();
        assert_eq!(payload.installed.len(), 1);
        assert_eq!(
            payload.installed[0].id,
            "superpowers@superpowers-marketplace"
        );
        assert_eq!(payload.available.len(), 1);
        assert_eq!(
            payload.available[0].plugin_id,
            "superpowers@claude-plugins-official"
        );
        assert_eq!(
            payload.available[0].marketplace_name.as_deref(),
            Some("claude-plugins-official")
        );
        assert_eq!(payload.available[0].version.as_deref(), Some("4.0.4"));
    }

    #[test]
    fn parse_claude_plugin_list_with_available_accepts_snake_case_fields() {
        let output = r#"
noise before json...
{
  "installed": [
    {
      "id": "foo@bar",
      "version": "1.0.0",
      "install_path": "/Users/a/.claude/plugins/cache/bar/foo/1.0.0",
      "installed_at": "2026-01-01T00:00:00Z",
      "last_updated": "2026-01-02T00:00:00Z"
    }
  ],
  "available": [
    {
      "plugin_id": "foo@bar",
      "marketplace_name": "bar",
      "version": "1.0.1"
    }
  ]
}
noise after json..."#;

        let payload: ClaudePluginListWithAvailable = parse_json_output(output).unwrap();
        assert_eq!(payload.installed.len(), 1);
        assert_eq!(
            payload.installed[0].install_path.as_deref(),
            Some("/Users/a/.claude/plugins/cache/bar/foo/1.0.0")
        );
        assert_eq!(payload.available.len(), 1);
        assert_eq!(payload.available[0].plugin_id, "foo@bar");
        assert_eq!(
            payload.available[0].marketplace_name.as_deref(),
            Some("bar")
        );
        assert_eq!(payload.available[0].version.as_deref(), Some("1.0.1"));
    }

    #[test]
    fn parse_claude_plugin_list_with_available_skips_unrelated_json() {
        let output = r#"
{"event":"progress","message":"fetching"}
{
  "installed": [
    {
      "id": "sample@market",
      "version": "1.0.0"
    }
  ],
  "available": [
    {
      "pluginId": "sample@market",
      "marketplaceName": "market",
      "version": "1.1.0"
    }
  ]
}
"#;

        let payload = parse_claude_plugin_list_with_available(output).unwrap();
        assert_eq!(payload.installed.len(), 1);
        assert_eq!(payload.installed[0].id, "sample@market");
        assert_eq!(payload.available.len(), 1);
        assert_eq!(payload.available[0].plugin_id, "sample@market");
        assert_eq!(payload.available[0].version.as_deref(), Some("1.1.0"));
    }
}

fn parse_marketplace_list_text(output: &str) -> Vec<ClaudeMarketplace> {
    let cleaned = strip_terminal_escapes(output);
    let mut results: Vec<ClaudeMarketplace> = Vec::new();
    let mut current_index: Option<usize> = None;

    for raw_line in cleaned.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim_start();

        if let Some(rest) = trimmed.strip_prefix('>') {
            let name = rest.trim().to_string();
            if name.is_empty() {
                continue;
            }
            let install_location = default_marketplace_install_location(&name);
            results.push(ClaudeMarketplace {
                name,
                source: None,
                repo: None,
                repository_url: None,
                install_location,
            });
            current_index = Some(results.len() - 1);
            continue;
        }

        let Some(idx) = current_index else { continue };
        if !trimmed.to_lowercase().starts_with("source:") {
            continue;
        }

        // Example:
        // Source: GitHub (anthropics/claude-plugins-official)
        // Source: URL (https://...)
        let after = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
        if after.is_empty() {
            continue;
        }

        let (source_text, paren) = match after.split_once('(') {
            Some((a, b)) => (a.trim(), Some(b.trim_end_matches(')').trim())),
            None => (after.trim(), None),
        };

        if !source_text.is_empty() {
            results[idx].source = Some(source_text.to_string());
        }

        if let Some(value) = paren {
            if !value.is_empty() {
                // GitHub: owner/repo; URL/Local: value as-is
                results[idx].repo = Some(value.to_string());
                results[idx].repository_url =
                    if value.starts_with("http://") || value.starts_with("https://") {
                        Some(value.to_string())
                    } else if value.contains('/') {
                        Some(format!("https://github.com/{}", value))
                    } else {
                        None
                    };
            }
        }
    }

    results
}

fn default_marketplace_install_location(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return None;
    }
    let home = dirs::home_dir()?;
    Some(
        home.join(".claude")
            .join("plugins")
            .join("marketplaces")
            .join(name)
            .to_string_lossy()
            .to_string(),
    )
}

fn git_output(args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    #[cfg(windows)]
    {
        // 避免 GUI 程序在 Windows 上弹出 git 控制台窗口
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().context("执行 git 命令失败")?;
    if !out.status.success() {
        anyhow::bail!("git 命令返回非零状态");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn parse_plugin_update_output(output: &str) -> String {
    let text = output.to_lowercase();
    if text.contains("already") && text.contains("latest") {
        return "already_latest".to_string();
    }
    if text.contains("success") && text.contains("updated") {
        return "updated".to_string();
    }
    if text.contains("updated") {
        return "updated".to_string();
    }
    "failed".to_string()
}

fn parse_marketplace_update_output(output: &str) -> bool {
    let text = output.to_lowercase();
    text.contains("successfully updated marketplace")
        || (text.contains("updated") && text.contains("marketplace") && !text.contains("failed"))
        || text.contains("already up to date")
}

#[derive(Debug)]
struct CommandOutcome {
    success: bool,
    already: bool,
}

fn parse_marketplace_add_output(output: &str) -> CommandOutcome {
    let text = output.to_lowercase();

    // 检查是否已存在（优先判断，因为 Claude Code 输出可能是 "Failed to add: already installed"）
    let already = text.contains("already")
        && (text.contains("marketplace")
            || text.contains("exists")
            || text.contains("added")
            || text.contains("installed"));

    // 如果已存在，直接视为成功
    if already {
        return CommandOutcome {
            success: true,
            already: true,
        };
    }

    // 检查是否有明确的失败信息
    let has_error = text.contains("error")
        || text.contains("failed")
        || text.contains("failure")
        || text.contains("unable to")
        || text.contains("could not");

    // 检查成功情况（排除错误情况）
    let success = !has_error
        && (text.contains("marketplace added")
            || text.contains("added marketplace")
            || text.contains("successfully added")
            || (text.contains("marketplace")
                && text.contains("added")
                && !text.contains("not added")));

    CommandOutcome {
        success,
        already: false,
    }
}

fn parse_plugin_install_output(output: &str) -> CommandOutcome {
    let text = output.to_lowercase();

    // 检查是否有明确的失败信息
    let has_error = text.contains("error")
        || text.contains("failed")
        || text.contains("failure")
        || text.contains("unable to")
        || text.contains("could not");

    // 检查是否未安装（否定）
    let not_installed = text.contains("not installed") || text.contains("not found");

    // 检查是否已存在
    let already = text.contains("already installed") || text.contains("already exists");

    // 检查成功情况（排除错误和否定情况）
    let success = !has_error
        && !not_installed
        && (already
        || text.contains("successfully installed")
        || text.contains("installation complete")
        || text.contains("install success")
        || text.contains("plugin installed")
        // 只有当 "installed" 不是在否定上下文中出现时才算成功
        || (text.contains("installed") && !text.contains("not installed") && !text.contains("isn't installed")));

    CommandOutcome { success, already }
}

fn parse_plugin_uninstall_output(output: &str) -> CommandOutcome {
    let text = output.to_lowercase();

    // 检查是否本来就未安装（可视为"成功"卸载）
    // 优先检查这个，因为 "not found" 比一般错误更具体
    let not_installed = text.contains("not installed")
        || text.contains("not found")
        || text.contains("doesn't exist")
        || (text.contains("not found") && text.contains("installed plugins"));

    // 检查是否有明确的失败信息（排除 "not found" 的情况）
    let has_error = !not_installed
        && (text.contains("error")
            || text.contains("failed")
            || text.contains("failure")
            || text.contains("unable to")
            || text.contains("could not"));

    // 检查成功情况
    let success = !has_error
        && (not_installed  // 本来就不存在，视为成功
        || text.contains("successfully uninstalled")
        || text.contains("uninstall success")
        || text.contains("plugin uninstalled")
        || text.contains("removed")
        || (text.contains("uninstalled") && !text.contains("not uninstalled")));

    CommandOutcome {
        success,
        already: not_installed,
    }
}

fn parse_marketplace_remove_output(output: &str) -> CommandOutcome {
    let text = output.to_lowercase();

    // 检查是否本来就不存在（可视为"成功"移除）
    let not_found = text.contains("not found")
        || text.contains("doesn't exist")
        || (text.contains("marketplace") && text.contains("not found"));

    // 检查是否有明确的失败信息（排除 "not found" 的情况）
    let has_error = !not_found
        && (text.contains("error")
            || text.contains("failed")
            || text.contains("failure")
            || text.contains("unable to")
            || text.contains("could not"));

    // 检查成功情况
    let success = !has_error
        && (not_found  // 本来就不存在，视为成功
        || text.contains("successfully removed")
        || text.contains("marketplace removed")
        || text.contains("removed marketplace")
        || text.contains("uninstalled")
        || (text.contains("removed") && !text.contains("not removed")));

    CommandOutcome {
        success,
        already: not_found,
    }
}

fn read_marketplace_manifest(repo_root: &Path) -> Result<Option<MarketplaceManifest>> {
    let manifest_path = repo_root.join(".claude-plugin").join("marketplace.json");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("无法读取 marketplace.json: {:?}", manifest_path))?;

    let manifest: MarketplaceManifest =
        serde_json::from_str(&content).context("解析 marketplace.json 失败")?;

    Ok(Some(manifest))
}

fn read_plugin_manifest(source_path: &Path) -> Result<PluginManifest> {
    let manifest_path = source_path.join(".claude-plugin").join("plugin.json");
    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("无法读取 plugin.json: {:?}", manifest_path))?;
    let manifest: PluginManifest =
        serde_json::from_str(&content).context("解析 plugin.json 失败")?;
    Ok(manifest)
}

fn normalize_source(source: &str) -> String {
    let mut trimmed = source.trim().to_string();
    if trimmed == "." || trimmed == "./" {
        return ".".to_string();
    }

    if trimmed.starts_with("./") {
        trimmed = trimmed.trim_start_matches("./").to_string();
    }

    trimmed = trimmed
        .trim_end_matches('/')
        .trim_end_matches('\\')
        .to_string();
    if trimmed.is_empty() {
        ".".to_string()
    } else {
        trimmed
    }
}

fn resolve_source_path(repo_root: &Path, source: &str) -> Result<PathBuf> {
    let normalized = normalize_source(source);
    if normalized == "." {
        return Ok(repo_root.to_path_buf());
    }

    let relative = PathBuf::from(&normalized);
    for component in relative.components() {
        match component {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("插件 source 路径不允许包含上级或绝对路径: {}", normalized);
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    Ok(repo_root.join(relative))
}

fn find_repo_root(extract_dir: &Path) -> Result<PathBuf> {
    for entry in std::fs::read_dir(extract_dir).context("无法读取解压目录")? {
        let entry = entry.context("无法读取目录条目")?;
        if entry.file_type()?.is_dir() {
            return Ok(entry.path());
        }
    }

    anyhow::bail!("未找到仓库根目录")
}

fn resolve_marketplace_plugins(
    repo_root: &Path,
    repo_url: &str,
    strict: bool,
) -> Result<Vec<ResolvedPlugin>> {
    let manifest = read_marketplace_manifest(repo_root)?
        .context("未找到 marketplace.json，无法自动安装插件")?;

    let mut resolved = Vec::new();
    for entry in manifest.plugins {
        let source = normalize_source(&entry.source);
        let source_path = resolve_source_path(repo_root, &source)?;
        if !source_path.exists() {
            anyhow::bail!("插件目录不存在: {}", source_path.to_string_lossy());
        }

        let plugin_manifest = match read_plugin_manifest(&source_path) {
            Ok(manifest) => Some(manifest),
            Err(e) => {
                if strict {
                    return Err(e);
                }
                None
            }
        };

        let name = plugin_manifest
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or_else(|| entry.name.clone());

        let mut plugin = Plugin::new(
            name,
            repo_url.to_string(),
            manifest.name.clone(),
            source.clone(),
        );

        plugin.description = plugin_manifest
            .as_ref()
            .and_then(|m| m.description.clone())
            .or(entry.description.clone());
        plugin.version = plugin_manifest
            .as_ref()
            .and_then(|m| m.version.clone())
            .or(entry.version.clone());
        plugin.author = plugin_manifest
            .as_ref()
            .and_then(|m| m.author.as_ref().and_then(|a| a.to_display()))
            .or(entry.author.as_ref().and_then(|a| a.to_display()));

        resolved.push(ResolvedPlugin {
            plugin,
            source_path,
        });
    }

    Ok(resolved)
}

fn merge_reports(reports: &[(Plugin, SecurityReport)], marketplace_name: &str) -> SecurityReport {
    let mut issues = Vec::new();
    let mut hard_triggers = Vec::new();
    let mut scanned_files = Vec::new();
    let mut skipped_files = Vec::new();
    let mut recommendations = HashSet::new();
    let mut score = 100;
    let mut blocked = false;
    let mut partial_scan = false;

    for (plugin, report) in reports {
        if report.score < score {
            score = report.score;
        }

        if report.blocked {
            blocked = true;
        }
        if report.partial_scan {
            partial_scan = true;
        }

        for issue in &report.issues {
            let mut updated = issue.clone();
            if let Some(path) = &issue.file_path {
                updated.file_path = Some(format!("{}/{}", plugin.name, path));
            }
            issues.push(updated);
        }

        for file in &report.scanned_files {
            scanned_files.push(format!("{}/{}", plugin.name, file));
        }
        for file in &report.skipped_files {
            skipped_files.push(format!("{}/{}", plugin.name, file));
        }

        for item in &report.hard_trigger_issues {
            hard_triggers.push(format!("[{}] {}", plugin.name, item));
        }

        for rec in &report.recommendations {
            recommendations.insert(rec.clone());
        }
    }

    SecurityReport {
        skill_id: format!("marketplace::{}", marketplace_name),
        score,
        level: SecurityLevel::from_score(score),
        issues,
        recommendations: recommendations.into_iter().collect(),
        blocked,
        hard_trigger_issues: hard_triggers,
        scanned_files,
        partial_scan: partial_scan || !skipped_files.is_empty(),
        skipped_files,
    }
}

fn localized_text(text: &LocalizedText, locale: &str) -> String {
    if locale.starts_with("zh") {
        text.zh.clone()
    } else {
        text.en.clone()
    }
}

fn author_to_display(author: &FeaturedMarketplaceOwner) -> Option<String> {
    match (&author.name, &author.email) {
        (Some(name), Some(email)) => Some(format!("{} <{}>", name, email)),
        (Some(name), None) => Some(name.clone()),
        (None, Some(email)) => Some(email.clone()),
        (None, None) => None,
    }
}
