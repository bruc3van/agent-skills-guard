use crate::models::{Skill, LOCAL_REPOSITORY_URL};
use crate::security::{calculate_checksum, scan_directory, scan_file, ScanOptions};
use crate::services::agent_tools::AgentTool;
use crate::services::link_fs;
use crate::services::migration::{MigrationManager, SkillAdoptionSummary, ToolSkillDir};
use crate::services::{Database, GitHubService};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SkillManager {
    db: Arc<Database>,
    github: GitHubService,
    skills_dir: PathBuf,
    cleanup_done: AtomicBool,
    installing: std::sync::Mutex<std::collections::HashSet<String>>,
}

const CACHE_LOCAL_PATH_PREFIX: &str = "__cache__:";
const STAGING_LOCAL_PATH_PREFIX: &str = "__staging__:";

fn build_synced_tool_state(
    source: &Path,
    skill_dir_name: &str,
    target_tools: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut linked_tool_ids: Vec<String> = Vec::new();
    let mut paths: Vec<String> = vec![source.to_string_lossy().to_string()];

    for tool_id in target_tools {
        let Some(tool) = AgentTool::from_id(tool_id) else {
            continue;
        };
        if tool == AgentTool::Agents || linked_tool_ids.iter().any(|id| id == tool.id()) {
            continue;
        }

        let Some(tool_dir) = tool.default_skills_dir() else {
            continue;
        };

        linked_tool_ids.push(tool.id().to_string());
        paths.push(tool_dir.join(skill_dir_name).to_string_lossy().to_string());
    }

    (linked_tool_ids, paths)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolLinkFailure {
    tool_id: String,
    reason: String,
}

impl ToolLinkFailure {
    fn new(tool_id: &str, reason: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            reason: reason.into(),
        }
    }
}

fn format_tool_link_failures(failures: &[ToolLinkFailure]) -> String {
    failures
        .iter()
        .map(|failure| format!("{} ({})", failure.tool_id, failure.reason))
        .collect::<Vec<_>>()
        .join("; ")
}

fn create_or_reuse_tool_link(
    source: &Path,
    link_path: &Path,
    tool_id: &str,
    source_checksum: Option<&str>,
    linked_tool_ids: &mut Vec<String>,
    link_failures: &mut Vec<ToolLinkFailure>,
) {
    if link_path.exists() || link_fs::is_dir_link(link_path) {
        if tool_skill_path_is_compatible_with_source(source, link_path, source_checksum) {
            log::info!("复用已存在的兼容工具路径 [{:?}]", link_path);
            push_unique_value(linked_tool_ids, tool_id.to_string());
        } else {
            let reason = format!("目标已存在同名但内容不同的技能: {:?}", link_path);
            log::warn!(
                "工具 '{}' 下已存在同名但内容不同的技能，不覆盖: {:?}",
                tool_id,
                link_path
            );
            link_failures.push(ToolLinkFailure::new(tool_id, reason));
        }
        return;
    }

    match link_fs::create_dir_link(source, link_path) {
        Ok(()) => {
            log::info!("链接创建成功 [{}]: {:?}", tool_id, link_path);
            push_unique_value(linked_tool_ids, tool_id.to_string());
        }
        Err(e) => {
            log::warn!("链接创建失败 [{}]: {}", tool_id, e);
            link_failures.push(ToolLinkFailure::new(tool_id, e.to_string()));
        }
    }
}

fn build_local_skill_id(checksum: &str, local_path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let mut normalized_path = local_path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized_path = normalized_path.to_ascii_lowercase();
    }
    let mut hasher = Sha256::new();
    hasher.update(checksum.as_bytes());
    hasher.update(b":");
    hasher.update(normalized_path.as_bytes());
    let digest = hex::encode(hasher.finalize());

    format!("local::{}", &digest[..16])
}

fn legacy_staging_path(local_path: &str) -> Option<&str> {
    local_path.strip_prefix(STAGING_LOCAL_PATH_PREFIX)
}

fn resolve_staging_dir(skill: &Skill) -> Option<PathBuf> {
    skill
        .staging_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| {
            skill
                .local_path
                .as_deref()
                .and_then(legacy_staging_path)
                .map(PathBuf::from)
        })
}

fn ensure_staging_path_is_contained(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("STAGING_PATH_RESOLVE_FAILED: {:?}", path))?;
    let cache_root = dirs::cache_dir()
        .context("CACHE_DIR_UNAVAILABLE")?
        .join("agent-skills-guard");
    let allowed_roots = [cache_root.join("repositories"), cache_root.join("staging")];

    let allowed = allowed_roots.into_iter().any(|root| {
        root.canonicalize()
            .map(|canonical_root| canonical.starts_with(canonical_root))
            .unwrap_or(false)
    });
    // 单元测试使用隔离的系统临时目录，不触碰用户缓存。
    #[cfg(test)]
    let allowed = allowed
        || std::env::temp_dir()
            .canonicalize()
            .map(|root| canonical.starts_with(root))
            .unwrap_or(false);
    if !allowed {
        anyhow::bail!("STAGING_PATH_NOT_ALLOWED: staging 路径不在应用缓存目录内")
    }
    Ok(canonical)
}

const UPDATE_SESSION_OWNER: &str = ".asg-update-owner";

/// Only remove a whole repository session when its app-created owner marker matches.
/// Legacy/shared staging paths retain the narrower per-skill cleanup behavior.
fn cleanup_update_staging(path: &Path, skill_id: &str) -> Result<()> {
    let staging_root = dirs::cache_dir()
        .context("CACHE_DIR_UNAVAILABLE")?
        .join("agent-skills-guard/staging");
    if let Ok(root) = staging_root.canonicalize() {
        for ancestor in path.ancestors() {
            let Ok(session) = ancestor.canonicalize() else {
                continue;
            };
            if session.parent() != Some(root.as_path())
                || !session
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("update-"))
            {
                continue;
            }
            let marker = session.join(UPDATE_SESSION_OWNER);
            if marker.exists() {
                if !std::fs::symlink_metadata(&marker)?.is_file()
                    || std::fs::read_to_string(&marker)? != skill_id
                {
                    anyhow::bail!("UPDATE_SESSION_OWNER_MISMATCH");
                }
                std::fs::remove_dir_all(session)?;
                return Ok(());
            }
        }
    }
    if path.exists() {
        std::fs::remove_dir_all(ensure_staging_path_is_contained(path)?)?;
    }
    Ok(())
}

fn normalize_path_for_compare(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

// Compare the directory entry, never the final symlink target. Deleting a registered
// link must not authorize deleting the directory to which it points.
fn registered_uninstall_path(skill: &Skill, requested: &Path) -> Result<PathBuf> {
    fn entry_key(path: &Path) -> Option<String> {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return None;
        }
        let name = path.file_name()?;
        let parent = path.parent()?.canonicalize().ok()?;
        Some(normalize_path_for_compare(&parent.join(name)))
    }
    let key = entry_key(requested).context("UNAUTHORIZED_PATH_REMOVAL: 无效的安装路径")?;
    for recorded in skill
        .local_path
        .iter()
        .chain(skill.local_paths.iter().flatten())
    {
        let path = Path::new(recorded);
        if entry_key(path).as_deref() == Some(key.as_str()) {
            return Ok(path.to_path_buf());
        }
    }
    anyhow::bail!("UNAUTHORIZED_PATH_REMOVAL: 目标路径不属于该技能")
}

fn paths_point_to_same_location(left: &Path, right: &Path) -> bool {
    if normalize_path_for_compare(left) == normalize_path_for_compare(right) {
        return true;
    }

    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => {
            normalize_path_for_compare(&left) == normalize_path_for_compare(&right)
        }
        _ => false,
    }
}

/// SKILL.md 校验和缓存条目：以 (修改时间, 大小) 作为失效依据
#[derive(Clone, PartialEq, Eq)]
struct ChecksumCacheEntry {
    modified: Option<std::time::SystemTime>,
    len: u64,
    checksum: String,
}

/// SKILL.md 校验和缓存。
///
/// `refresh_existing_tool_links_for_skill` 会对「每个技能 × 每个工具目录」
/// 调用一次校验和比对，一次 `get_installed_skills` 就是 O(N×T) 次
/// 文件读取 + SHA256。100 个技能 / 5 个工具目录即上千次哈希，是切换标签页
/// 卡顿的主要来源。这里按 (mtime, size) 缓存，文件未变更时零 IO。
static SKILL_MD_CHECKSUM_CACHE: std::sync::LazyLock<ChecksumCache> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn hash_bytes(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

type ChecksumCache = std::sync::Mutex<std::collections::HashMap<PathBuf, ChecksumCacheEntry>>;

/// 缓存注入版本，便于测试使用独立缓存（全局缓存是进程级状态，
/// 在并行测试下无法可靠断言）。
fn skill_md_checksum_with_cache(skill_dir: &Path, cache: &ChecksumCache) -> Option<String> {
    let path = skill_dir.join("SKILL.md");

    // 取不到元数据（文件不存在 / 无权限）时不使用缓存：
    // 文件已被删除就应当如实返回 None，而不是给出陈旧校验和。
    let Ok(metadata) = std::fs::metadata(&path) else {
        return std::fs::read(&path).ok().map(|c| hash_bytes(&c));
    };
    let modified = metadata.modified().ok();
    let len = metadata.len();

    if let Ok(cache) = cache.lock() {
        if let Some(entry) = cache.get(&path) {
            if entry.modified == modified && entry.len == len {
                return Some(entry.checksum.clone());
            }
        }
    }

    let checksum = hash_bytes(&std::fs::read(&path).ok()?);

    if let Ok(mut cache) = cache.lock() {
        cache.insert(
            path,
            ChecksumCacheEntry {
                modified,
                len,
                checksum: checksum.clone(),
            },
        );
    }

    Some(checksum)
}

fn skill_md_checksum(skill_dir: &Path) -> Option<String> {
    skill_md_checksum_with_cache(skill_dir, &SKILL_MD_CHECKSUM_CACHE)
}

/// 清空 SKILL.md 校验和缓存。
///
/// (mtime, size) 在绝大多数文件系统上足以判定变更，但安装 / 更新 / 卸载
/// 会主动覆写技能目录，此时不依赖时间戳精度、直接失效整个缓存最稳妥。
/// 缓存本身很小，`clear()` 的代价可以忽略。
fn invalidate_skill_md_checksum_cache() {
    if let Ok(mut cache) = SKILL_MD_CHECKSUM_CACHE.lock() {
        cache.clear();
    }
}

fn tool_skill_path_is_compatible_with_source(
    source: &Path,
    tool_skill_path: &Path,
    source_checksum: Option<&str>,
) -> bool {
    if paths_point_to_same_location(source, tool_skill_path) {
        return true;
    }

    let Some(tool_checksum) = skill_md_checksum(tool_skill_path) else {
        return false;
    };
    // A third-party install may be a real directory rather than a link. Treat it as the
    // same skill only when SKILL.md matches; this is compatibility detection, not trust.
    let expected_checksum =
        skill_md_checksum(source).or_else(|| source_checksum.map(str::to_owned));

    expected_checksum.as_deref() == Some(tool_checksum.as_str())
}

fn path_is_inside_dir_resolving_links(path: &Path, dir: &Path) -> bool {
    if path.starts_with(dir) {
        return true;
    }

    match (std::fs::canonicalize(path), std::fs::canonicalize(dir)) {
        (Ok(path), Ok(dir)) => path.starts_with(dir),
        _ => false,
    }
}

fn path_is_same_or_inside_dir(path: &Path, dir: &Path) -> bool {
    let normalized_path = normalize_path_for_compare(path);
    let normalized_dir = normalize_path_for_compare(dir);

    normalized_path == normalized_dir
        || normalized_path.starts_with(&format!("{}/", normalized_dir))
}

fn install_base_conflicting_tool(install_base_dir: &Path) -> Option<AgentTool> {
    for tool in AgentTool::all() {
        if tool == AgentTool::Agents {
            continue;
        }

        let Some(tool_skills_dir) = tool.default_skills_dir() else {
            continue;
        };

        if path_is_same_or_inside_dir(install_base_dir, &tool_skills_dir) {
            return Some(tool);
        }
    }

    None
}

/// A custom base directory is user-controlled and the staged directory name originates
/// from repository content. Never replace an unrelated pre-existing child directory.
fn ensure_custom_install_target_is_owned(
    skill: &Skill,
    final_install_dir: &Path,
    _custom_install_base: bool,
) -> Result<()> {
    if std::fs::symlink_metadata(final_install_dir).is_err() {
        return Ok(());
    }

    let tracked = skill
        .local_path
        .as_deref()
        .into_iter()
        .chain(
            skill
                .local_paths
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(String::as_str),
        )
        .any(|path| paths_point_to_same_location(Path::new(path), final_install_dir));

    if !tracked {
        anyhow::bail!(
            "INSTALL_TARGET_EXISTS: 自定义安装目标 {:?} 已存在且不属于该技能",
            final_install_dir
        );
    }

    Ok(())
}

#[cfg(test)]
fn ensure_update_clear_has_backup(
    target_exists: bool,
    should_clear: bool,
    backup_available: bool,
) -> Result<()> {
    if target_exists && should_clear && !backup_available {
        anyhow::bail!(
            "UPDATE_BACKUP_REQUIRED: 目标目录仍存在但没有可用备份，已拒绝破坏性清理；staging 已保留，可关闭占用目录的程序后重试"
        );
    }
    Ok(())
}

fn find_tool_id_for_scan_dir(
    scan_dir: &Path,
    dir_to_tool: &HashMap<PathBuf, String>,
) -> Option<String> {
    if let Some(tool_id) = dir_to_tool.get(scan_dir) {
        return Some(tool_id.clone());
    }

    dir_to_tool
        .iter()
        .find(|(dir, _)| paths_point_to_same_location(scan_dir, dir))
        .map(|(_, tool_id)| tool_id.clone())
}

fn push_unique_value(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn default_tool_dirs() -> Vec<(PathBuf, String)> {
    AgentTool::all()
        .into_iter()
        .filter_map(|tool| {
            tool.default_skills_dir()
                .map(|dir| (dir, tool.id().to_string()))
        })
        .collect()
}

fn default_tool_skill_dirs() -> Vec<ToolSkillDir> {
    AgentTool::all()
        .into_iter()
        .filter_map(|tool| {
            tool.default_skills_dir().map(|path| ToolSkillDir {
                tool_id: tool.id().to_string(),
                path,
            })
        })
        .collect()
}

fn skill_candidate_paths(skill: &Skill) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(path) = &skill.source_path {
        push_unique_value(&mut paths, path.clone());
    }
    if let Some(path) = &skill.local_path {
        push_unique_value(&mut paths, path.clone());
    }
    if let Some(local_paths) = &skill.local_paths {
        for path in local_paths {
            push_unique_value(&mut paths, path.clone());
        }
    }
    paths
}

fn refresh_existing_tool_links_for_skill(skill: &Skill, tool_dirs: &[(PathBuf, String)]) -> Skill {
    if !skill.installed {
        return skill.clone();
    }

    let candidate_paths = skill_candidate_paths(skill);
    let Some(source_path) = candidate_paths
        .iter()
        .find(|path| PathBuf::from(path).exists())
        .or_else(|| candidate_paths.first())
    else {
        return skill.clone();
    };

    let source = PathBuf::from(source_path);
    let Some(skill_dir_name) = source.file_name() else {
        return skill.clone();
    };

    let mut refreshed = skill.clone();
    let mut local_paths = Vec::new();
    for candidate_path in &candidate_paths {
        let candidate = PathBuf::from(candidate_path);
        if candidate.exists()
            && tool_skill_path_is_compatible_with_source(
                &source,
                &candidate,
                skill.checksum.as_deref(),
            )
        {
            push_unique_value(&mut local_paths, candidate_path.clone());
        }
    }

    let mut linked_tools = Vec::new();

    for (tool_dir, tool_id) in tool_dirs {
        let tool_skill_path = tool_dir.join(skill_dir_name);
        if !tool_skill_path.exists() {
            continue;
        }
        if !tool_skill_path_is_compatible_with_source(
            &source,
            &tool_skill_path,
            skill.checksum.as_deref(),
        ) {
            continue;
        }

        push_unique_value(
            &mut local_paths,
            tool_skill_path.to_string_lossy().to_string(),
        );
        if tool_id != AgentTool::Agents.id() {
            push_unique_value(&mut linked_tools, tool_id.clone());
        }
    }

    if local_paths.is_empty() {
        return skill.clone();
    }

    let current_source_exists = skill
        .source_path
        .as_deref()
        .map(|path| PathBuf::from(path).exists())
        .unwrap_or(false);
    if !current_source_exists {
        if let Some(replacement_source) = local_paths.first().cloned() {
            refreshed.source_path = Some(replacement_source);
        } else {
            log::warn!(
                "技能 '{}' 的 source_path {:?} 不存在，且未找到可用替代路径",
                skill.name,
                skill.source_path
            );
        }
    }
    refreshed.local_path = local_paths.first().cloned();
    refreshed.local_paths = Some(local_paths);
    refreshed.linked_tools = linked_tools;
    refreshed
}

fn installed_tool_state_changed(previous: &Skill, refreshed: &Skill) -> bool {
    previous.source_path != refreshed.source_path
        || previous.local_path != refreshed.local_path
        || previous.local_paths != refreshed.local_paths
        || previous.linked_tools != refreshed.linked_tools
}

fn restore_installation_backup(backup_path: &Path, final_install_dir: &Path) -> Result<()> {
    if final_install_dir.exists() {
        let metadata = std::fs::symlink_metadata(final_install_dir)
            .with_context(|| format!("无法读取残留安装目录: {:?}", final_install_dir))?;
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            std::fs::remove_dir_all(final_install_dir)
                .with_context(|| format!("无法清理残留安装目录: {:?}", final_install_dir))?;
        } else {
            std::fs::remove_file(final_install_dir)
                .with_context(|| format!("无法清理残留安装路径: {:?}", final_install_dir))?;
        }
    }

    rename_with_retry(backup_path, final_install_dir).with_context(|| {
        format!(
            "无法还原安装备份: {:?} -> {:?}",
            backup_path, final_install_dir
        )
    })
}

fn resolve_update_target_install_dir(raw: &Path) -> PathBuf {
    if !link_fs::is_dir_link(raw) {
        return raw.to_path_buf();
    }

    let linked_target = match link_fs::read_dir_link_target(raw) {
        Ok(target) => target,
        Err(_) => return std::fs::canonicalize(raw).unwrap_or_else(|_| raw.to_path_buf()),
    };

    let resolved = if linked_target.is_absolute() {
        linked_target
    } else {
        raw.parent()
            .map(|parent| parent.join(&linked_target))
            .unwrap_or(linked_target)
    };

    std::fs::canonicalize(&resolved).unwrap_or(resolved)
}

fn resolve_update_install_paths(raw: &Path) -> (PathBuf, PathBuf) {
    (raw.to_path_buf(), resolve_update_target_install_dir(raw))
}

impl SkillManager {
    pub fn new(db: Arc<Database>) -> Self {
        let skills_dir = Self::get_skills_directory();

        Self {
            db,
            github: GitHubService::new(),
            skills_dir,
            cleanup_done: AtomicBool::new(false),
            installing: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// 获取统一源 skills 安装目录（~/.agents/skills）
    fn get_skills_directory() -> PathBuf {
        AgentTool::Agents.default_skills_dir().unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".agents").join("skills")
        })
    }

    fn create_temp_install_dir(&self, base: &Path, name: &str) -> Result<tempfile::TempDir> {
        tempfile::Builder::new()
            .prefix(&format!(".{name}.tmp-"))
            .tempdir_in(base)
            .context("无法创建临时安装目录")
    }

    fn replace_installation_directory(
        &self,
        prepared_dir: &Path,
        final_install_dir: &Path,
    ) -> Result<()> {
        let mut backup_dir = None;

        if final_install_dir.exists() {
            let file_name = final_install_dir
                .file_name()
                .and_then(|name| name.to_str())
                .context("INVALID_SKILL_DIR_NAME")?;
            let parent = final_install_dir.parent().context("无效的安装目录")?;
            let backup_path =
                parent.join(format!(".{}.backup-{}", file_name, uuid::Uuid::new_v4()));

            rename_with_retry(final_install_dir, &backup_path).with_context(|| {
                format!(
                    "无法备份现有技能目录，请关闭占用该目录的程序: {:?}",
                    final_install_dir
                )
            })?;
            backup_dir = Some(backup_path);
        }

        match rename_with_retry(prepared_dir, final_install_dir) {
            Ok(()) => {
                if let Some(backup_path) = backup_dir {
                    if let Err(error) = std::fs::remove_dir_all(&backup_path) {
                        log::warn!("清理安装备份目录失败: {:?}, 错误: {}", backup_path, error);
                    }
                }
                Ok(())
            }
            Err(error) => {
                let mut restore_error = None;
                if let Some(backup_path) = backup_dir {
                    // 先尝试将备份还原到原位，再清理临时目录
                    if let Err(restore_err) =
                        restore_installation_backup(&backup_path, final_install_dir)
                    {
                        log::error!(
                            "还原安装备份失败: {:?} -> {:?}, 错误: {}",
                            backup_path,
                            final_install_dir,
                            restore_err
                        );
                        restore_error = Some(restore_err);
                    }
                }
                if prepared_dir.exists() {
                    let _ = std::fs::remove_dir_all(prepared_dir);
                }
                if let Some(restore_error) = restore_error {
                    Err(anyhow::anyhow!(
                        "无法替换技能目录: {:?} -> {:?}, 错误: {}；同时无法还原原安装目录: {}",
                        prepared_dir,
                        final_install_dir,
                        error,
                        restore_error
                    ))
                } else {
                    Err(anyhow::anyhow!(
                        "无法替换技能目录: {:?} -> {:?}, 错误: {}",
                        prepared_dir,
                        final_install_dir,
                        error
                    ))
                }
            }
        }
    }

    fn apply_scan_report(skill: &mut Skill, report: &crate::models::SecurityReport) {
        skill.security_score = Some(report.score);
        skill.security_level = Some(report.level.as_str().to_string());
        skill.security_issues = Some(report.issues.clone());
        // 落库前剥离 scanned_files：前端从不读取，却会随每次技能列表查询
        // 反序列化并经 IPC 传输（每份报告最多 2000 条路径）
        skill.security_report = Some(report.clone().without_scanned_file_list());
        skill.scanned_at = Some(Utc::now());
    }

    fn enforce_installable_report(
        &self,
        report: &crate::models::SecurityReport,
        operation: &str,
        allow_partial_scan: bool,
    ) -> Result<()> {
        if report.blocked || !report.hard_trigger_issues.is_empty() {
            let mut error_msg = format!("SECURITY_CHECK_BLOCKED: {}\n", operation);
            for (idx, issue) in report.hard_trigger_issues.iter().enumerate() {
                error_msg.push_str(&format!("{}. {}\n", idx + 1, issue));
            }
            anyhow::bail!(error_msg);
        }

        if report.partial_scan && !allow_partial_scan {
            let mut error_msg = format!("SECURITY_PARTIAL_SCAN_BLOCKED: {}\n", operation);
            if report.skipped_files.is_empty() {
                error_msg.push_str("1. 扫描过程中存在被截断或跳过的文件\n");
            } else {
                for (idx, file) in report.skipped_files.iter().enumerate() {
                    error_msg.push_str(&format!("{}. {}\n", idx + 1, file));
                }
            }
            anyhow::bail!(error_msg);
        }

        Ok(())
    }

    fn rescan_skill_directory_for_confirmation(
        &self,
        dir: &Path,
        skill_id: &str,
        allow_partial_scan: bool,
    ) -> Result<crate::models::SecurityReport> {
        let locale = rust_i18n::locale();
        let report = scan_directory(
            dir.to_str().context("INVALID_SKILL_DIR_PATH")?,
            skill_id,
            &locale,
            ScanOptions {
                skip_readme: false,
                ..Default::default()
            },
        )?;
        self.enforce_installable_report(&report, "安装或更新技能", allow_partial_scan)?;
        Ok(report)
    }

    /// 下载并分析 skill，返回文件内容和安全报告
    pub async fn download_and_analyze(
        &self,
        skill: &mut Skill,
    ) -> Result<(Vec<u8>, crate::models::SecurityReport)> {
        let (owner, repo) = crate::models::Repository::from_github_url(&skill.repository_url)?;
        let content = self
            .github
            .download_skill_markdown(&owner, &repo, &skill.file_path)
            .await?;
        let (name, description) = self
            .github
            .parse_skill_frontmatter(std::str::from_utf8(&content)?)?;
        skill.name = name;
        skill.description = description;
        let locale = rust_i18n::locale();
        let report = scan_file(
            &String::from_utf8_lossy(&content),
            "SKILL.md",
            &locale,
            ScanOptions::default(),
        )?;
        skill.checksum = Some(calculate_checksum(&content));
        Ok((content, report))
    }

    /// 安装 skill 到本地（旧入口，内部委托给 prepare + confirm）
    pub async fn install_skill(
        &self,
        skill_id: &str,
        install_path: Option<String>,
        allow_partial_scan: bool,
    ) -> Result<()> {
        let locale = rust_i18n::locale();
        self.prepare_skill_installation(skill_id, &locale).await?;
        self.confirm_skill_installation(skill_id, install_path, allow_partial_scan, Vec::new())?;
        Ok(())
    }

    /// 准备安装技能：扫描缓存中的技能，但不复制文件，不标记为已安装
    /// 返回扫描报告供前端判断是否需要用户确认
    pub async fn prepare_skill_installation(
        &self,
        skill_id: &str,
        locale: &str,
    ) -> Result<crate::models::security::SecurityReport> {
        use anyhow::Context;

        log::info!("Preparing installation for skill: {}", skill_id);

        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        // 获取仓库记录
        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("REPOSITORY_NOT_FOUND")?
            .clone();

        // 确保仓库缓存存在
        let cache_path = if let Some(existing_cache_path) = &repo.cache_path {
            // 验证缓存路径是否存在
            let cache_path_buf = PathBuf::from(existing_cache_path);
            if cache_path_buf.exists() {
                existing_cache_path.clone()
            } else {
                // 缓存路径不存在，重新下载
                log::warn!("缓存路径不存在，重新下载仓库: {:?}", cache_path_buf);
                self.download_and_cache_repository(&repo.id, &skill.repository_url)
                    .await?
            }
        } else {
            // 仓库缓存不存在，自动下载
            log::info!("仓库缓存不存在，自动下载: {}", skill.repository_url);
            self.download_and_cache_repository(&repo.id, &skill.repository_url)
                .await?
        };

        // 定位缓存中的技能目录
        log::info!("从仓库缓存定位技能: {:?}", cache_path);
        let skill_cache_dir =
            self.locate_skill_in_cache(PathBuf::from(&cache_path).as_path(), &skill.file_path)?;

        log::info!("在缓存中找到技能目录: {:?}", skill_cache_dir);
        let content = std::fs::read_to_string(skill_cache_dir.join("SKILL.md"))?;
        let (name, description) = self.github.parse_skill_frontmatter(&content)?;
        skill.name = name;
        skill.description = description;
        skill.checksum = Some(calculate_checksum(content.as_bytes()));

        // 直接扫描缓存中的技能目录
        let scan_report = scan_directory(
            skill_cache_dir.to_str().context("INVALID_SKILL_DIR_PATH")?,
            &skill.id,
            locale,
            ScanOptions {
                skip_readme: false,
                ..Default::default()
            },
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        // 更新 skill 安全信息到数据库（但不标记为已安装）
        Self::apply_scan_report(&mut skill, &scan_report);
        // 使用 staging_path 存储临时缓存路径，不污染 local_path
        skill.staging_path = Some(skill_cache_dir.to_string_lossy().to_string());

        // 解析路径感知的 installed_commit_sha：以「上次仓库下载时记录的 commit SHA」
        // （而非真实的当前远端 HEAD）为参考，查询最近触及该 skill 子目录的提交 SHA。
        // 子目录 skill 若直接记录仓库 HEAD，会与后续 check_skill_update 用的路径过滤结果
        // 不一致，导致永远报告有更新。此处只写入候选记录，confirm 成功后才更新安装记录；
        // 不可用时回退到 cached_repo_sha 自身。
        let cached_repo_sha = {
            let repos = self.db.get_repositories()?;
            repos
                .iter()
                .find(|r| r.url == skill.repository_url)
                .and_then(|r| r.cached_commit_sha.clone())
        };
        if let Some(cached) = cached_repo_sha.as_deref() {
            let resolved = self
                .resolve_path_aware_commit_sha(&skill.repository_url, &skill.file_path, cached)
                .await;
            skill.installed_commit_sha = Some(resolved);
        }

        // Candidate metadata belongs to preparation, never to the active installed version.
        self.db.save_skill_preparation(&skill)?;
        let mut active = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;
        active.staging_path = skill.staging_path.clone();
        self.db.save_skill(&active)?;

        // prepare 阶段必须把 partial_scan 报告返回给 UI，用户才能看到跳过文件并点击
        // “谨慎安装”。真正写入前的 confirm 会重新扫描，并要求显式 allow_partial_scan。
        self.enforce_installable_report(&scan_report, "准备安装技能", true)?;

        log::info!("Skill prepared successfully, scanned from cache, awaiting user confirmation");
        Ok(scan_report)
    }

    /// 下载并缓存仓库
    async fn download_and_cache_repository(&self, repo_id: &str, repo_url: &str) -> Result<String> {
        use anyhow::Context;

        log::info!("Downloading and caching repository: {}", repo_url);

        // 解析 GitHub URL
        let (owner, repo_name) = crate::models::Repository::from_github_url(repo_url)?;

        // 获取缓存基础目录
        let cache_base_dir = dirs::cache_dir()
            .context("CACHE_DIR_UNAVAILABLE")?
            .join("agent-skills-guard")
            .join("repositories");

        // 下载仓库压缩包并解压
        let (extract_dir, commit_sha) = self
            .github
            .download_repository_archive(&owner, &repo_name, &cache_base_dir)
            .await
            .context("下载仓库压缩包失败")?;

        let cache_path_str = extract_dir.to_string_lossy().to_string();

        // 更新数据库缓存信息
        self.db
            .update_repository_cache(repo_id, &cache_path_str, Utc::now(), Some(&commit_sha))
            .context("更新仓库缓存信息失败")?;

        log::info!("Repository cached successfully: {}", cache_path_str);

        Ok(cache_path_str)
    }

    /// 解析「路径感知」commit SHA：以 `ref_sha` 为参考，查询该 skill 子目录最近一次
    /// 被触及的提交 SHA。任意失败均回退为 `ref_sha` 本身，保证调用方拿到一个可用值。
    ///
    /// `ref_sha` 通常是「上次下载仓库时记录的 commit SHA」或刚刚下载得到的新 SHA，
    /// 不一定等于真实的当前远端 HEAD —— 故参数名不再叫 `head_sha`。
    ///
    /// 这是 prepare/update 共用的解析入口。
    async fn resolve_path_aware_commit_sha(
        &self,
        repository_url: &str,
        file_path: &str,
        ref_sha: &str,
    ) -> String {
        let (owner, repo_name) = match crate::models::Repository::from_github_url(repository_url) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("无法解析仓库 URL，回退为 ref={}：{}", ref_sha, e);
                return ref_sha.to_string();
            }
        };

        match self
            .github
            .fetch_latest_commit_sha_for_path(&owner, &repo_name, file_path, Some(ref_sha))
            .await
        {
            Ok(Some(path_aware)) => {
                log::info!(
                    "解析路径感知 commit SHA: {} (ref={}, path={})",
                    path_aware,
                    ref_sha,
                    file_path
                );
                path_aware
            }
            Ok(None) => {
                log::warn!(
                    "路径感知 SHA 查询无结果，回退为 ref={} (path={})",
                    ref_sha,
                    file_path
                );
                ref_sha.to_string()
            }
            Err(e) => {
                log::warn!("路径感知 SHA 查询失败，回退为 ref={}：{}", ref_sha, e);
                ref_sha.to_string()
            }
        }
    }

    /// 把 `installed_commit_sha` 写回为「规范的 path-aware SHA」。
    ///
    /// 用于 [`crate::services::github::SkillUpdateStatus::UpToDate`] 携带 `canonical_sha`
    /// 时的自愈：旧版数据可能把仓库 HEAD 当作 installed SHA 存入，导致每次更新检查都需要
    /// 额外的回退查询。命中即修，避免长期付出 2× API 成本。
    ///
    /// 注意：只有当 skill 已安装且本地记录的 SHA 与 `new_sha` 实际不同时才写回，
    /// 避免无谓的 db 写。
    pub fn migrate_installed_commit_sha(&self, skill_id: &str, new_sha: &str) -> Result<bool> {
        use anyhow::Context;

        if new_sha.is_empty() {
            return Ok(false);
        }

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        if skill.installed_commit_sha.as_deref() == Some(new_sha) {
            return Ok(false);
        }

        log::info!(
            "迁移 skill {} 的 installed_commit_sha: {:?} -> {}",
            skill_id,
            skill.installed_commit_sha,
            new_sha
        );
        skill.installed_commit_sha = Some(new_sha.to_string());
        self.db.save_skill(&skill)?;
        Ok(true)
    }

    /// 在仓库缓存中定位技能目录
    fn locate_skill_in_cache(
        &self,
        cache_path: &std::path::Path,
        skill_file_path: &str,
    ) -> Result<PathBuf> {
        // 找到仓库根目录（cache_path 指向 extracted/ 目录）
        let repo_root = self.find_repo_root_in_cache(cache_path)?;

        // 构建技能在缓存中的路径
        let skill_cache_path = if skill_file_path == "." {
            repo_root.clone()
        } else {
            repo_root.join(skill_file_path.replace('\\', "/"))
        };

        if !skill_cache_path.exists() {
            anyhow::bail!("缓存中未找到技能目录: {:?}", skill_cache_path);
        }

        Ok(skill_cache_path)
    }

    /// 找到GitHub zipball解压后的根目录
    fn find_repo_root_in_cache(&self, extract_dir: &std::path::Path) -> Result<PathBuf> {
        use anyhow::Context;

        // GitHub zipball解压后会有一个 {owner}-{repo}-{commit}/ 目录
        for entry in std::fs::read_dir(extract_dir).context("无法读取解压目录")? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // 第一个目录就是仓库根目录
                return Ok(path);
            }
        }

        anyhow::bail!("REPOSITORY_ROOT_NOT_FOUND")
    }

    /// 递归复制目录
    /// counter: 文件计数器，传入 &mut 变量可统计复制的文件数
    fn copy_dir_recursive(
        &self,
        src: &std::path::Path,
        dst: &std::path::Path,
        counter: &mut usize,
    ) -> Result<()> {
        use anyhow::Context;

        if !dst.exists() {
            std::fs::create_dir_all(dst).context(format!("无法创建目标目录: {:?}", dst))?;
        }

        for entry in std::fs::read_dir(src).context(format!("无法读取源目录: {:?}", src))? {
            let entry = entry.context(format!("读取目录项失败: {:?}", src))?;
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);
            let file_type = entry
                .file_type()
                .context(format!("无法获取文件类型: {:?}", src_path))?;

            if file_type.is_symlink() {
                log::warn!("跳过符号链接（源目录中不允许）: {:?}", src_path);
                continue;
            } else if file_type.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path, counter)?;
            } else if file_type.is_file() {
                // 确保目标文件的父目录存在
                if let Some(parent) = dst_path.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent)
                            .context(format!("无法创建文件父目录: {:?}", parent))?;
                    }
                }

                match std::fs::copy(&src_path, &dst_path) {
                    Ok(bytes) => {
                        *counter += 1;
                        log::debug!("已复制文件: {:?} ({} bytes)", file_name, bytes);
                    }
                    Err(e) => {
                        let error_msg = if e.raw_os_error() == Some(5) {
                            format!(
                                "复制文件失败（拒绝访问）\n文件: {:?}\n\n可能原因：\n1. 目标文件正在被其他程序使用\n2. 文件被设置为只读\n3. 权限不足\n4. 杀毒软件拦截\n\n建议：\n1. 关闭可能打开该文件的程序\n2. 检查文件是否为只读\n3. 以管理员权限运行\n\n原始错误: {}",
                                file_name, e
                            )
                        } else {
                            format!(
                                "复制文件失败\n源: {:?}\n目标: {:?}\n错误: {}",
                                src_path, dst_path, e
                            )
                        };
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }
            }
        }

        Ok(())
    }

    #[cfg(test)]
    fn clear_update_staging_state_best_effort(&self, skill: &mut Skill) {
        if skill
            .local_path
            .as_deref()
            .and_then(legacy_staging_path)
            .is_some()
        {
            skill.local_path = skill
                .local_paths
                .as_ref()
                .and_then(|paths| paths.last().cloned());
        }
        skill.staging_path = None;

        if let Err(error) = self.db.save_skill(skill) {
            log::warn!(
                "清理更新 staging 状态失败，skill={} error={}",
                skill.name,
                error
            );
        }
    }

    /// 确认安装技能：从缓存复制到目标路径，标记为已安装
    /// target_tools: 要同步链接的工具 id 列表（除 "agents" 外的工具）
    pub fn confirm_skill_installation(
        &self,
        skill_id: &str,
        install_path: Option<String>,
        allow_partial_scan: bool,
        target_tools: Vec<String>,
    ) -> Result<()> {
        // 即将改写技能目录，先失效 SKILL.md 校验和缓存
        invalidate_skill_md_checksum_cache();
        log::info!("Confirming installation for skill: {}", skill_id);

        {
            let mut installing = self.installing.lock().unwrap_or_else(|e| e.into_inner());
            if installing.contains(skill_id) {
                anyhow::bail!("SKILL_INSTALL_IN_PROGRESS");
            }
            installing.insert(skill_id.to_string());
        }

        let result = self.confirm_skill_installation_inner(
            skill_id,
            install_path,
            allow_partial_scan,
            target_tools,
        );
        self.installing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(skill_id);
        result
    }

    /// Inner implementation of confirm_skill_installation (called after installing guard is set)
    fn confirm_skill_installation_inner(
        &self,
        skill_id: &str,
        install_path: Option<String>,
        allow_partial_scan: bool,
        target_tools: Vec<String>,
    ) -> Result<()> {
        use anyhow::Context;
        use std::path::PathBuf;

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        // 获取缓存中的技能路径（优先从 staging_path 读取，向后兼容 __cache__: 前缀）
        let cache_dir = skill
            .staging_path
            .as_deref()
            .map(PathBuf::from)
            .or_else(|| {
                skill
                    .local_path
                    .as_deref()
                    .and_then(|path| path.strip_prefix(CACHE_LOCAL_PATH_PREFIX))
                    .map(PathBuf::from)
            })
            .context("SKILL_NOT_PREPARED: 请重新执行准备安装")?;
        let cache_dir = ensure_staging_path_is_contained(&cache_dir)?;

        // 优先使用 prepare 阶段写入的路径感知 SHA；缺失时回退到仓库 HEAD（向后兼容
        // 旧路径与未经过 prepare 的极端情况）。
        let repositories = self.db.get_repositories()?;
        let repo = repositories.iter().find(|r| r.url == skill.repository_url);
        let candidate = self
            .db
            .get_skill_preparation(skill_id)?
            .filter(|c| c.staging_path == skill.staging_path);
        let commit_sha = candidate
            .as_ref()
            .and_then(|c| c.installed_commit_sha.clone())
            .or_else(|| repo.and_then(|r| r.cached_commit_sha.clone()));
        if let Some(candidate) = &candidate {
            skill.name = candidate.name.clone();
            skill.description = candidate.description.clone();
            skill.checksum = candidate.checksum.clone();
        }

        // 确定最终安装路径
        let install_base_dir = if let Some(user_path) = &install_path {
            let user_dir = PathBuf::from(user_path);
            // 校验：用户路径不能指向工具自己的 skills 目录（会导致自引用）。
            // ~/.agents/skills 是统一源目录，允许作为安装基准路径。
            if let Some(tool) = install_base_conflicting_tool(&user_dir) {
                anyhow::bail!(
                    "INSTALL_PATH_CONFLICT: 安装路径 {:?} 与工具 '{}' 的技能目录冲突",
                    user_dir,
                    tool.id()
                );
            }
            user_dir
        } else {
            self.skills_dir.clone()
        };

        // 获取技能目录名
        let skill_dir_name = cache_dir.file_name().context("INVALID_SKILL_DIR_NAME")?;
        let final_install_dir = install_base_dir.join(skill_dir_name);

        ensure_custom_install_target_is_owned(&skill, &final_install_dir, install_path.is_some())?;

        // 确保目标基础目录存在
        std::fs::create_dir_all(&install_base_dir).context("无法创建目标目录")?;

        let skill_dir_name = skill_dir_name.to_string_lossy().to_string();
        let prepared = self.create_temp_install_dir(&install_base_dir, &skill_dir_name)?;
        let prepared_dir = prepared.path();

        self.rescan_skill_directory_for_confirmation(&cache_dir, &skill.id, allow_partial_scan)?;

        // 从缓存复制到目标路径
        log::info!(
            "Copying skill from cache {:?} to {:?}",
            cache_dir,
            prepared_dir
        );
        let mut files_copied = 0;
        self.copy_dir_recursive(&cache_dir, &prepared_dir, &mut files_copied)?;

        log::info!(
            "Copied {} files from cache to install directory",
            files_copied
        );

        let scan_report = self.rescan_skill_directory_for_confirmation(
            prepared_dir,
            &skill.id,
            allow_partial_scan,
        )?;
        let baseline = super::content_manifest::manifest(prepared_dir)?;
        self.replace_installation_directory(prepared_dir, &final_install_dir)?;

        // 更新源路径（~/.agents/skills/<name>）
        let source_path_str = final_install_dir.to_string_lossy().to_string();

        // 为其他工具创建 Junction/symlink 链接
        let mut linked_tool_ids: Vec<String> = Vec::new();
        let mut link_failures: Vec<ToolLinkFailure> = Vec::new();
        let non_agent_tools: Vec<AgentTool> = target_tools
            .iter()
            .filter_map(|id| AgentTool::from_id(id))
            .filter(|t| t != &AgentTool::Agents)
            .collect();

        for tool in &non_agent_tools {
            if let Some(tool_dir) = tool.default_skills_dir() {
                let link_path = tool_dir.join(skill_dir_name.as_str());
                create_or_reuse_tool_link(
                    &final_install_dir,
                    &link_path,
                    tool.id(),
                    None,
                    &mut linked_tool_ids,
                    &mut link_failures,
                );
            }
        }

        // 更新 local_path（向后兼容）
        skill.local_path = Some(source_path_str.clone());

        // 用户选择了非 agents 工具但全部失败时，安装主体仍已完成。
        // 保存源路径后返回部分成功错误，让前端刷新状态并提示用户稍后重试同步。
        if !non_agent_tools.is_empty() && linked_tool_ids.is_empty() {
            skill.source_path = Some(source_path_str.clone());
            skill.local_paths = Some(vec![source_path_str.clone()]);
            skill.linked_tools = Vec::new();
            skill.is_local_only = false;
            skill.installed = true;
            skill.installed_at = Some(Utc::now());
            skill.installed_commit_sha = commit_sha;
            skill.staging_path = None;
            Self::apply_scan_report(&mut skill, &scan_report);
            self.db
                .save_installed_skill_with_baseline(&skill, &baseline)?;
            super::skill_reports::refresh_cross_skill_reports(&self.db)?;
            self.invalidate_installed_cache();
            let detail = if link_failures.is_empty() {
                non_agent_tools
                    .iter()
                    .map(|tool| tool.id())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                format_tool_link_failures(&link_failures)
            };
            anyhow::bail!("LINK_CREATION_ALL_FAILED: {}", detail);
        }
        skill.local_path = Some(source_path_str.clone());

        // local_paths 包含源 + 所有成功创建的链接路径（与 linked_tool_ids 保持一致）
        let (linked_tool_ids, paths) = build_synced_tool_state(
            &final_install_dir,
            skill_dir_name.as_str(),
            &linked_tool_ids,
        );
        skill.local_paths = Some(paths);

        // 设置新字段
        skill.source_path = Some(source_path_str);
        skill.linked_tools = linked_tool_ids;
        skill.is_local_only = false;

        // 标记为已安装
        skill.installed = true;
        skill.installed_at = Some(Utc::now());
        skill.installed_commit_sha = commit_sha;
        skill.staging_path = None; // 清除临时缓存路径
        Self::apply_scan_report(&mut skill, &scan_report);

        self.db
            .save_installed_skill_with_baseline(&skill, &baseline)?;
        super::skill_reports::refresh_cross_skill_reports(&self.db)?;

        log::info!("Skill installation confirmed: {}", skill.name);
        self.invalidate_installed_cache();
        Ok(())
    }

    /// 取消安装技能：清除准备阶段的数据（不删除缓存）
    pub fn cancel_skill_installation(&self, skill_id: &str) -> Result<()> {
        use anyhow::Context;

        log::info!("Canceling installation for skill: {}", skill_id);

        if self
            .installing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(skill_id)
        {
            anyhow::bail!("SKILL_INSTALL_IN_PROGRESS");
        }

        let skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        // 注意：不删除缓存中的文件，因为缓存是共享的仓库缓存
        // 只清除数据库中的准备阶段信息

        let mut skill = skill;
        skill.staging_path = None;
        self.db.clear_skill_preparation(skill_id)?;

        self.db.save_skill(&skill)?;

        log::info!("Skill installation canceled: {}", skill.name);
        self.invalidate_installed_cache();
        Ok(())
    }

    /// 清理 DB 中残留的 __cache__: / __staging__: 临时路径
    /// 在 prepare 后用户未 confirm/cancel 就关闭 App 时，这些路径会残留
    pub fn cleanup_stale_prepare_paths(&self) -> Result<usize> {
        let skills = self.db.get_skills()?;
        let mut cleaned = 0usize;

        for skill in &skills {
            let mut needs_update = false;
            let mut updated = skill.clone();

            // 检查 local_path 是否为残留临时路径
            if let Some(local_path) = &skill.local_path {
                let is_temp = local_path.starts_with(CACHE_LOCAL_PATH_PREFIX)
                    || local_path.starts_with(STAGING_LOCAL_PATH_PREFIX);
                if is_temp {
                    let actual_path = local_path
                        .strip_prefix(CACHE_LOCAL_PATH_PREFIX)
                        .or_else(|| local_path.strip_prefix(STAGING_LOCAL_PATH_PREFIX))
                        .unwrap_or(local_path);

                    if !PathBuf::from(actual_path).exists() {
                        if !skill.installed {
                            updated.local_path = None;
                            updated.security_score = None;
                            updated.security_level = None;
                            updated.security_issues = None;
                            updated.security_report = None;
                            updated.scanned_at = None;
                        } else {
                            updated.local_path = skill
                                .local_paths
                                .as_ref()
                                .and_then(|paths| paths.last().cloned());
                        }
                        needs_update = true;
                        log::info!(
                            "清理残留临时路径 (local_path): skill={}, path={}",
                            skill.name,
                            local_path
                        );
                    }
                }
            }

            // 检查 local_paths 中是否包含残留临时路径

            // 检查 staging_path 是否残留
            if let Some(ref staging) = skill.staging_path {
                if !PathBuf::from(staging).exists() {
                    updated.staging_path = None;
                    if !skill.installed {
                        updated.local_path = None;
                        updated.security_score = None;
                        updated.security_level = None;
                        updated.security_issues = None;
                        updated.security_report = None;
                        updated.scanned_at = None;
                    }
                    needs_update = true;
                    log::info!(
                        "清理残留 staging_path: skill={}, path={}",
                        skill.name,
                        staging
                    );
                }
            }
            if let Some(local_paths) = &skill.local_paths {
                let cleaned_paths: Vec<String> = local_paths
                    .iter()
                    .filter(|p| {
                        let is_temp = p.starts_with(CACHE_LOCAL_PATH_PREFIX)
                            || p.starts_with(STAGING_LOCAL_PATH_PREFIX);
                        if !is_temp {
                            return true; // 保留非临时路径
                        }
                        let actual = p
                            .strip_prefix(CACHE_LOCAL_PATH_PREFIX)
                            .or_else(|| p.strip_prefix(STAGING_LOCAL_PATH_PREFIX))
                            .unwrap_or(p.as_str());
                        let keep = PathBuf::from(actual).exists();
                        if !keep {
                            log::info!(
                                "清理残留临时路径 (local_paths): skill={}, path={}",
                                skill.name,
                                p
                            );
                        }
                        keep
                    })
                    .cloned()
                    .collect();
                if cleaned_paths.len() < local_paths.len() {
                    updated.local_paths = Some(cleaned_paths);
                    needs_update = true;
                }
            }

            if needs_update {
                self.db.save_skill(&updated)?;
                cleaned += 1;
            }
        }

        // Second pass: delete orphaned __cache__ skills where cache dir no longer exists
        let orphaned: Vec<String> = skills
            .iter()
            .filter(|s| {
                if s.installed {
                    return false;
                }
                // 检查 staging_path 残留
                if let Some(staging) = &s.staging_path {
                    return !PathBuf::from(staging).exists();
                }
                // 向后兼容：检查 __cache__:/__staging__: 前缀
                if let Some(local_path) = &s.local_path {
                    if local_path.starts_with(CACHE_LOCAL_PATH_PREFIX)
                        || local_path.starts_with(STAGING_LOCAL_PATH_PREFIX)
                    {
                        let actual_path = local_path
                            .strip_prefix(CACHE_LOCAL_PATH_PREFIX)
                            .or_else(|| local_path.strip_prefix(STAGING_LOCAL_PATH_PREFIX))
                            .unwrap_or(local_path);
                        return !PathBuf::from(actual_path).exists();
                    }
                }
                false
            })
            .map(|s| s.id.clone())
            .collect();

        for skill_id in orphaned {
            log::info!("清理孤立的缓存技能记录: {}", skill_id);
            let _ = self.db.delete_skill(&skill_id);
        }

        Ok(cleaned)
    }

    /// 卸载 skill
    pub fn uninstall_skill(&self, skill_id: &str) -> Result<()> {
        // 即将删除技能目录，先失效 SKILL.md 校验和缓存
        invalidate_skill_md_checksum_cache();

        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        let mut errors: Vec<String> = Vec::new();

        // 分离链接和源：先删所有链接（避免源先删后链接变成悬空 Junction）
        // 第一遍：删除链接（Junction/symlink）
        if let Some(local_paths) = &skill.local_paths {
            for local_path in local_paths {
                let path = PathBuf::from(local_path);
                if link_fs::is_dir_link(&path) {
                    if let Err(e) = link_fs::remove_dir_link(&path) {
                        let msg = format!("删除技能链接失败: {:?}, 错误: {}", path, e);
                        log::warn!("{}", msg);
                        errors.push(msg);
                    }
                }
            }
        }

        // 第二遍：删除真实目录/文件（source 及降级拷贝）
        if let Some(local_paths) = &skill.local_paths {
            for local_path in local_paths {
                let path = PathBuf::from(local_path);
                if link_fs::is_dir_link(&path) {
                    continue; // 已在第一遍处理
                }
                if path.exists() {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            let msg = format!("删除技能目录失败: {:?}, 错误: {}", path, e);
                            log::warn!("{}", msg);
                            errors.push(msg);
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        let msg = format!("删除技能文件失败: {:?}, 错误: {}", path, e);
                        log::warn!("{}", msg);
                        errors.push(msg);
                    }
                }
            }
        }

        // 向后兼容:如果 local_paths 为空,尝试删除 local_path
        if skill.local_paths.is_none() || skill.local_paths.as_ref().unwrap().is_empty() {
            if let Some(local_path) = &skill.local_path {
                let path = PathBuf::from(local_path);
                if link_fs::is_dir_link(&path) {
                    if let Err(e) = link_fs::remove_dir_link(&path) {
                        errors.push(format!("无法删除技能链接: {}", e));
                    }
                } else if path.exists() {
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            errors.push(format!("无法删除技能目录: {}", e));
                        }
                    } else if let Err(e) = std::fs::remove_file(&path) {
                        errors.push(format!("无法删除技能文件: {}", e));
                    }
                }
            }
        }

        // 只有所有路径都成功删除后才清空安装记录。
        // 部分失败时保留原元数据，否则残留文件会失去追踪，用户也无法重试卸载。
        if !errors.is_empty() {
            log::warn!("Skill uninstall completed with errors: {}", skill.name);
            self.invalidate_installed_cache();
            anyhow::bail!("UNINSTALL_PARTIAL_FAILURE: {}", errors.join("; "))
        }

        // 更新数据库
        skill.installed = false;
        skill.installed_at = None;
        skill.local_path = None;
        skill.local_paths = None;
        skill.source_path = None;
        skill.linked_tools = Vec::new();
        skill.is_local_only = false;

        self.db.save_skill(&skill).context("更新数据库失败")?;
        super::skill_reports::refresh_cross_skill_reports(&self.db)?;

        log::info!("Skill uninstalled successfully: {}", skill.name);
        self.invalidate_installed_cache();
        Ok(())
    }

    /// 卸载特定路径的技能
    pub fn uninstall_skill_path(&self, skill_id: &str, path_to_remove: &str) -> Result<()> {
        // 即将删除技能目录，先失效 SKILL.md 校验和缓存
        invalidate_skill_md_checksum_cache();

        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        // 删除指定路径的文件：链接用 remove_dir_link，真实目录用 remove_dir_all
        let mut errors: Vec<String> = Vec::new();
        let path = registered_uninstall_path(&skill, Path::new(path_to_remove))?;
        if link_fs::is_dir_link(&path) {
            if let Err(e) = link_fs::remove_dir_link(&path) {
                errors.push(format!("无法删除技能链接: {}", e));
            }
        } else if path.exists() {
            if path.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    errors.push(format!("无法删除技能目录: {}", e));
                }
            } else if let Err(e) = std::fs::remove_file(&path) {
                errors.push(format!("无法删除技能文件: {}", e));
            }
        }

        // 文件系统删除失败时必须保留原有安装元数据，否则 UI/数据库会把仍然存在或
        // 仅被部分删除的路径误报为已卸载，后续也失去重试和修复入口。
        if !errors.is_empty() {
            log::warn!(
                "Skill path uninstall failed; preserving metadata: {} from {}",
                skill.name,
                path_to_remove
            );
            self.invalidate_installed_cache();
            anyhow::bail!("UNINSTALL_PATH_PARTIAL_FAILURE: {}", errors.join("; "))
        }

        // 从 local_paths 中移除该路径（使用规范化比较，兼容 Windows 大小写/分隔符差异）
        let normalized_remove = normalize_path_for_compare(&path);
        if let Some(mut paths) = skill.local_paths.clone() {
            paths.retain(|p| normalize_path_for_compare(&PathBuf::from(p)) != normalized_remove);

            if paths.is_empty() {
                // 如果没有剩余路径
                if skill.is_local_only && skill.repository_url == LOCAL_REPOSITORY_URL {
                    // 本地技能：直接删除 DB 记录（与 reconcile_stale_skills 行为一致）
                    self.db
                        .delete_skill(&skill.id)
                        .context("删除技能记录失败")?;
                    log::info!("已删除本地技能记录: {}", skill.name);
                    super::skill_reports::refresh_cross_skill_reports(&self.db)?;
                    return Ok(());
                }
                // 市场技能：标记为未安装
                skill.installed = false;
                skill.installed_at = None;
                skill.local_path = None;
                skill.local_paths = None;
            } else {
                // 还有其他路径,更新列表
                skill.local_paths = Some(paths.clone());
                skill.local_path = paths.last().cloned(); // 更新为最后一个路径
            }
        }

        self.db.save_skill(&skill).context("更新数据库失败")?;
        super::skill_reports::refresh_cross_skill_reports(&self.db)?;

        log::info!(
            "Skill path uninstalled: {} from {}",
            skill.name,
            path_to_remove
        );
        self.invalidate_installed_cache();
        Ok(())
    }

    /// 获取所有 skills
    pub fn get_all_skills(&self) -> Result<Vec<Skill>> {
        self.get_all_skills_from_dirs(&default_tool_skill_dirs())
    }

    fn cleanup_stale_prepare_paths_once(&self) {
        if self.cleanup_done.load(Ordering::Relaxed) {
            return;
        }

        if let Err(e) = self.cleanup_stale_prepare_paths() {
            log::warn!("清理残留临时路径时出错（忽略）: {}", e);
        }
        self.cleanup_done.store(true, Ordering::Relaxed);
    }

    fn get_all_skills_from_dirs(&self, tool_skill_dirs: &[ToolSkillDir]) -> Result<Vec<Skill>> {
        self.cleanup_stale_prepare_paths_once();

        let tool_dirs = tool_skill_dirs
            .iter()
            .map(|tool_dir| (tool_dir.path.clone(), tool_dir.tool_id.clone()))
            .collect::<Vec<_>>();

        self.refresh_installed_tool_links_from_dirs(&tool_dirs)?;

        let adoption_summary = self.adopt_existing_skills_for_scan(tool_skill_dirs)?;
        if adoption_summary.discovered > 0
            || adoption_summary.created > 0
            || adoption_summary.updated > 0
        {
            log::info!(
                "All skills refresh adopted existing skills: discovered={}, created={}, updated={}",
                adoption_summary.discovered,
                adoption_summary.created,
                adoption_summary.updated
            );
        }

        self.db.get_skills()
    }

    /// 获取已安装的 skills
    pub fn get_installed_skills(&self) -> Result<Vec<Skill>> {
        self.get_installed_skills_from_dirs(&default_tool_skill_dirs())
    }

    /// 强制按磁盘实际状态刷新所有已安装技能的工具链接，并返回刷新后的已安装列表。
    ///
    /// 与 `get_installed_skills` 的区别：后者在每次读取时也会 reconcile，
    /// 但应用内更新后旧进程已退出、新进程前端可能未触发启动期 reconcile；
    /// 这个方法提供一个明确的「按当前磁盘状态重建 linked_tools」入口，
    /// 供更新安装后、手动重新扫描等场景显式调用，保证 Claude Code / Codex 等软链能被点亮。
    pub fn refresh_skill_links(&self) -> Result<Vec<Skill>> {
        let tool_dirs = default_tool_dirs();
        self.refresh_skill_links_from_dirs(&tool_dirs)
    }

    /// `refresh_skill_links` 的可测试变体，允许传入显式工具目录。
    fn refresh_skill_links_from_dirs(&self, tool_dirs: &[(PathBuf, String)]) -> Result<Vec<Skill>> {
        let refreshed = self.refresh_installed_tool_links_from_dirs(tool_dirs)?;
        log::info!(
            "Forcibly refreshed tool-link state for {} skills",
            refreshed
        );
        self.get_installed_skills_from_dirs(
            &tool_dirs
                .iter()
                .map(|(path, tool_id)| ToolSkillDir {
                    tool_id: tool_id.clone(),
                    path: path.clone(),
                })
                .collect::<Vec<_>>(),
        )
    }

    fn get_installed_skills_from_dirs(
        &self,
        tool_skill_dirs: &[ToolSkillDir],
    ) -> Result<Vec<Skill>> {
        // 清理残留的 __cache__: / __staging__: 临时路径（仅首次调用时执行）
        self.cleanup_stale_prepare_paths_once();

        let tool_dirs = tool_skill_dirs
            .iter()
            .map(|tool_dir| (tool_dir.path.clone(), tool_dir.tool_id.clone()))
            .collect::<Vec<_>>();

        // 这一步不可省略：它把 source_path / local_paths 重新指向磁盘上真实存在
        // 的副本，adoption 才能把发现的磁盘分组匹配到既有记录，而不是新建一条
        // 重复记录（见 installed_skills_reuse_existing_record_when_source_is_missing）。
        // 它与下方循环确实做同样的链接比对，但比对中最昂贵的
        // 「读 SKILL.md + SHA256」已由 SKILL_MD_CHECKSUM_CACHE 命中缓存，
        // 第二轮几乎零 IO。
        self.refresh_installed_tool_links_from_dirs(&tool_dirs)?;

        let adoption_summary = self.adopt_existing_skills_for_scan(tool_skill_dirs)?;
        if adoption_summary.discovered > 0
            || adoption_summary.created > 0
            || adoption_summary.updated > 0
        {
            log::info!(
                "Installed skills refresh adopted existing skills: discovered={}, created={}, updated={}",
                adoption_summary.discovered,
                adoption_summary.created,
                adoption_summary.updated
            );
        }

        let skills = self.db.get_skills()?;
        let mut installed = Vec::new();

        for skill in skills.into_iter().filter(|s| s.installed) {
            let refreshed = refresh_existing_tool_links_for_skill(&skill, &tool_dirs);
            if installed_tool_state_changed(&skill, &refreshed) {
                self.db.save_skill(&refreshed)?;
            }
            installed.push(refreshed);
        }

        Ok(installed)
    }

    fn invalidate_installed_cache(&self) {
        // Installed skills are reconciled on every read; keep this hook for mutation call sites.
    }

    fn refresh_installed_tool_links(&self) -> Result<usize> {
        let tool_dirs = default_tool_dirs();
        self.refresh_installed_tool_links_from_dirs(&tool_dirs)
    }

    fn refresh_installed_tool_links_from_dirs(
        &self,
        tool_dirs: &[(PathBuf, String)],
    ) -> Result<usize> {
        let skills = self.db.get_skills()?;
        let mut refreshed_count = 0usize;

        for skill in skills.into_iter().filter(|s| s.installed) {
            let refreshed = refresh_existing_tool_links_for_skill(&skill, tool_dirs);
            if installed_tool_state_changed(&skill, &refreshed) {
                self.db.save_skill(&refreshed)?;
                refreshed_count += 1;
            }
        }

        Ok(refreshed_count)
    }

    fn adopt_existing_skills_for_scan(
        &self,
        tool_dirs: &[ToolSkillDir],
    ) -> Result<SkillAdoptionSummary> {
        MigrationManager::new(Arc::clone(&self.db)).adopt_existing_skills_from_dirs(tool_dirs)
    }

    /// 扫描所有工具 skill 目录，导入未追踪的技能；通过 realpath 去重，避免链接和源重复导入
    pub fn scan_local_skills(&self) -> Result<Vec<Skill>> {
        use std::collections::HashSet;

        let adoption_summary = self.adopt_existing_skills_for_scan(&default_tool_skill_dirs())?;
        if adoption_summary.discovered > 0 {
            log::info!(
                "Manual scan adopted existing skills before scanning: discovered={}, created={}, updated={}",
                adoption_summary.discovered,
                adoption_summary.created,
                adoption_summary.updated
            );
        }

        // 所有扫描到的技能
        let mut scanned_skills = Vec::new();
        // 新导入的技能（用于日志）
        let mut imported_skills = Vec::new();
        // realpath -> scanned_skills 索引，用于去重并关联多工具
        let mut seen_real_paths: HashMap<PathBuf, usize> = HashMap::new();

        // 获取当前数据库中的所有技能（用于去重和提取路径）
        let existing_skills = self.db.get_skills()?;

        // 预建索引，避免 O(N*M) 线性扫描
        let mut index_by_path: HashMap<String, usize> = HashMap::new();
        let mut index_by_dirname: HashMap<String, Vec<usize>> = HashMap::new();
        let mut index_by_checksum: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, skill) in existing_skills.iter().enumerate() {
            if let Some(ref lp) = skill.local_path {
                index_by_path.insert(normalize_path_for_compare(&PathBuf::from(lp)), i);
            }
            if let Some(ref lps) = skill.local_paths {
                for lp in lps {
                    index_by_path.insert(normalize_path_for_compare(&PathBuf::from(lp)), i);
                }
            }
            if let Some(ref lp) = skill.local_path {
                if let Some(dir_name) = PathBuf::from(lp).file_name().and_then(|n| n.to_str()) {
                    index_by_dirname
                        .entry(dir_name.to_string())
                        .or_default()
                        .push(i);
                }
            }
            if let Some(ref checksum) = skill.checksum {
                index_by_checksum
                    .entry(checksum.clone())
                    .or_default()
                    .push(i);
            }
        }

        // 1. 获取所有 unique 的 local_path 父目录
        let mut scan_dirs: HashSet<PathBuf> = HashSet::new();

        // 从已安装技能的 local_path 提取父目录（跳过旧版临时缓存/staging标记）
        for skill in &existing_skills {
            if let Some(local_path) = &skill.local_path {
                if local_path.starts_with(CACHE_LOCAL_PATH_PREFIX)
                    || local_path.starts_with(STAGING_LOCAL_PATH_PREFIX)
                {
                    continue;
                }
                if let Some(parent) = PathBuf::from(local_path).parent() {
                    scan_dirs.insert(parent.to_path_buf());
                }
            }
        }

        // 2. 添加所有工具的 skill 目录（确保始终扫描）
        let mut dir_to_tool: HashMap<PathBuf, String> = HashMap::new();
        for tool in AgentTool::all() {
            if let Some(dir) = tool.default_skills_dir() {
                dir_to_tool.insert(dir.clone(), tool.id().to_string());
                scan_dirs.insert(dir);
            }
        }

        log::info!("Will scan {} directories for local skills", scan_dirs.len());

        // 3. 扫描所有目录
        for scan_dir in scan_dirs {
            let scan_dir_tool = find_tool_id_for_scan_dir(&scan_dir, &dir_to_tool);
            if !scan_dir.exists() {
                log::debug!("Skipping non-existent directory: {:?}", scan_dir);
                continue;
            }

            log::info!("Scanning directory: {:?}", scan_dir);

            // 遍历技能目录
            if let Ok(entries) = std::fs::read_dir(&scan_dir) {
                'entry: for entry in entries.flatten() {
                    let path = entry.path();

                    // 只处理目录
                    if !path.is_dir() {
                        continue;
                    }

                    // 检查是否包含 SKILL.md
                    let skill_md_path = path.join("SKILL.md");
                    if !skill_md_path.exists() {
                        continue;
                    }

                    // 通过 realpath 去重：同一技能出现在多个工具目录时，关联所有工具
                    let real_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                    let path_str = path.to_string_lossy().to_string();
                    if let Some(&idx) = seen_real_paths.get(&real_path) {
                        // 同一技能已在本次扫描中处理过，将当前目录的工具关联上去
                        let existing: &mut Skill = &mut scanned_skills[idx];
                        if let Some(ref tool_id) = scan_dir_tool {
                            push_unique_value(&mut existing.linked_tools, tool_id.clone());
                            log::info!(
                                "关联工具 '{}' 到技能 '{}'（来自 {:?}）",
                                tool_id,
                                existing.name,
                                path
                            );
                        }
                        let paths = existing.local_paths.get_or_insert_with(Vec::new);
                        if !paths.contains(&path_str) {
                            paths.push(path_str.clone());
                        }
                        let mut should_save = true;
                        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
                            let (skill_name, skill_description) =
                                self.parse_frontmatter(&content).unwrap_or_else(|_| {
                                    (existing.name.clone(), existing.description.clone())
                                });
                            if existing.repository_url == LOCAL_REPOSITORY_URL {
                                existing.name = skill_name;
                                existing.description = skill_description;
                                existing.file_path = path_str.clone();
                            }
                        } else if scan_dir_tool.is_none() {
                            should_save = false;
                        }
                        if should_save {
                            let _ = self.db.save_skill(existing);
                        }
                        continue;
                    }
                    // 也检查 DB 中已有记录（前次扫描导入的）
                    // 找到匹配后必须 continue 'entry，否则会继续走新 skill 创建逻辑
                    if let Some(ref tool_id) = scan_dir_tool {
                        for db_skill in &existing_skills {
                            if !db_skill.installed {
                                continue;
                            }
                            let db_real = db_skill
                                .local_path
                                .as_deref()
                                .and_then(|p| std::fs::canonicalize(p).ok());
                            if db_real.as_deref() == Some(&real_path) {
                                let mut updated = db_skill.clone();
                                push_unique_value(&mut updated.linked_tools, tool_id.clone());
                                log::info!(
                                    "关联工具 '{}' 到已有技能 '{}'（来自 {:?}）",
                                    tool_id,
                                    updated.name,
                                    path
                                );
                                if let Some(ref mut paths) = updated.local_paths {
                                    if !paths.contains(&path_str) {
                                        paths.push(path_str.clone());
                                    }
                                } else {
                                    updated.local_paths = Some(vec![path_str.clone()]);
                                }
                                if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
                                    let (skill_name, skill_description) =
                                        self.parse_frontmatter(&content).unwrap_or_else(|_| {
                                            (updated.name.clone(), updated.description.clone())
                                        });
                                    if updated.repository_url == LOCAL_REPOSITORY_URL {
                                        updated.name = skill_name;
                                        updated.description = skill_description;
                                        updated.file_path = path_str.clone();
                                    }
                                    let checksum = calculate_checksum(content.as_bytes());
                                    if updated.checksum.as_deref() != Some(checksum.as_str()) {
                                        updated.checksum = Some(checksum);
                                    }
                                    log::info!(
                                        "刷新已有本地技能 '{}' 的元数据（来自 {:?}）",
                                        updated.name,
                                        skill_md_path
                                    );
                                }
                                let _ = self.db.save_skill(&updated);
                                scanned_skills.push(updated);
                                seen_real_paths.insert(real_path, scanned_skills.len() - 1);
                                // 跳过新 skill 创建逻辑，该路径已在 DB 中记录
                                continue 'entry;
                            }
                        }
                    }
                    seen_real_paths.insert(real_path, scanned_skills.len());

                    // 读取 SKILL.md 内容
                    match std::fs::read_to_string(&skill_md_path) {
                        Ok(content) => {
                            // 计算 checksum
                            let checksum = calculate_checksum(content.as_bytes());

                            // 解析 frontmatter 获取元数据（用于展示/更新）
                            let (skill_name, skill_description) =
                                self.parse_frontmatter(&content).unwrap_or_else(|_| {
                                    (
                                        path.file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string(),
                                        None,
                                    )
                                });

                            // 检查是否已存在（按 local_path 和 local_paths 去重，避免目录不变但名称变化导致重复导入）
                            let local_path_str = path.to_string_lossy().to_string();
                            let normalized_scan_path = normalize_path_for_compare(&path);
                            let existing_by_path: Vec<Skill> = index_by_path
                                .get(&normalized_scan_path)
                                .map(|&i| vec![existing_skills[i].clone()])
                                .unwrap_or_default();

                            if existing_by_path.len() > 1 {
                                log::warn!(
                                "Found {} duplicated skills with same local_path={}, will update the first one",
                                existing_by_path.len(),
                                local_path_str
                            );
                            }

                            if let Some(mut existing_skill) = existing_by_path.into_iter().next() {
                                // 确保安装状态/路径一致
                                if !existing_skill.installed {
                                    existing_skill.installed = true;
                                    existing_skill.installed_at = Some(Utc::now());
                                }
                                if existing_skill.local_path.as_deref()
                                    != Some(local_path_str.as_str())
                                {
                                    existing_skill.local_path = Some(local_path_str.clone());
                                }

                                // 关联当前目录对应的工具
                                if let Some(ref tool_id) = scan_dir_tool {
                                    if !existing_skill.linked_tools.contains(tool_id) {
                                        existing_skill.linked_tools.push(tool_id.clone());
                                    }
                                }

                                // 更新 checksum（基于 SKILL.md 内容）
                                let checksum_changed =
                                    existing_skill.checksum.as_deref() != Some(checksum.as_str());
                                if checksum_changed {
                                    existing_skill.checksum = Some(checksum.clone());
                                }

                                // 仅对本地导入的技能（repository_url == local）更新 name/description/file_path
                                // 避免覆盖市场技能的元数据来源（仓库扫描/市场配置）
                                if existing_skill.repository_url == LOCAL_REPOSITORY_URL {
                                    existing_skill.name = skill_name;
                                    existing_skill.description = skill_description;
                                    existing_skill.file_path = local_path_str.clone();
                                }

                                // 仅在 checksum 变化时重新扫描，避免每次扫描全量安全检查的性能开销
                                if checksum_changed {
                                    let path_str = match path.to_str() {
                                        Some(s) => s,
                                        None => {
                                            log::warn!("跳过非 UTF-8 路径: {:?}", path);
                                            continue;
                                        }
                                    };
                                    let locale = rust_i18n::locale();
                                    let report = scan_directory(
                                        path_str,
                                        &existing_skill.id,
                                        &locale,
                                        ScanOptions {
                                            skip_readme: false,
                                            ..Default::default()
                                        },
                                    )?;

                                    Self::apply_scan_report(&mut existing_skill, &report);
                                }

                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // 二次匹配：按目录名为 local::* 技能查找已有记录，避免技能被移动+编辑后产生重复
                            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                            let existing_by_dir =
                                index_by_dirname.get(dir_name.as_ref()).and_then(|indices| {
                                    indices
                                        .iter()
                                        .find(|&&i| {
                                            existing_skills[i].id.starts_with("local::")
                                                && !existing_skills[i].installed
                                        })
                                        .map(|&i| &existing_skills[i])
                                });
                            if let Some(mut existing_skill) = existing_by_dir.cloned() {
                                log::info!(
                                    "Reusing existing local skill '{}' for moved directory {:?} -> {:?}",
                                    existing_skill.id,
                                    existing_skill.local_path,
                                    path
                                );
                                let checksum_changed =
                                    existing_skill.checksum.as_deref() != Some(checksum.as_str());
                                existing_skill.local_path = Some(local_path_str.clone());
                                existing_skill.local_paths = Some(vec![local_path_str.clone()]);
                                existing_skill.source_path = Some(local_path_str.clone());
                                existing_skill.installed = true;
                                existing_skill.installed_at = Some(Utc::now());
                                existing_skill.name = skill_name;
                                existing_skill.description = skill_description;
                                existing_skill.file_path = local_path_str.clone();

                                if checksum_changed {
                                    existing_skill.checksum = Some(checksum);
                                    let path_str = match path.to_str() {
                                        Some(s) => s,
                                        None => {
                                            log::warn!("跳过非 UTF-8 路径: {:?}", path);
                                            continue;
                                        }
                                    };
                                    let locale = rust_i18n::locale();
                                    let report = scan_directory(
                                        path_str,
                                        &existing_skill.id,
                                        &locale,
                                        ScanOptions {
                                            skip_readme: false,
                                            ..Default::default()
                                        },
                                    )?;
                                    Self::apply_scan_report(&mut existing_skill, &report);
                                }

                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // Fallback: match by checksum for renamed directories
                            let existing_by_checksum =
                                index_by_checksum.get(&checksum).and_then(|indices| {
                                    indices
                                        .iter()
                                        .find(|&&i| {
                                            !existing_skills[i].installed
                                                && existing_skills[i].repository_url == "local"
                                        })
                                        .map(|&i| &existing_skills[i])
                                });
                            if let Some(mut existing_skill) = existing_by_checksum.cloned() {
                                log::info!(
                                    "Reusing existing local skill '{}' by checksum for renamed directory {:?} -> {:?}",
                                    existing_skill.id,
                                    existing_skill.local_path,
                                    path
                                );
                                // Checksum unchanged (matched by checksum), skip full security rescan
                                existing_skill.local_path = Some(local_path_str.clone());
                                existing_skill.local_paths = Some(vec![local_path_str.clone()]);
                                existing_skill.source_path = Some(local_path_str.clone());
                                existing_skill.installed = true;
                                existing_skill.installed_at = Some(Utc::now());
                                existing_skill.checksum = Some(checksum);
                                existing_skill.name = skill_name;
                                existing_skill.description = skill_description;
                                existing_skill.file_path = local_path_str.clone();

                                // Checksum matched → content unchanged, reuse existing security report
                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // 生成技能 ID
                            let skill_id = build_local_skill_id(&checksum, &path);

                            // 扫描整个技能目录
                            let path_str = match path.to_str() {
                                Some(s) => s,
                                None => {
                                    log::warn!("跳过非 UTF-8 路径: {:?}", path);
                                    continue;
                                }
                            };
                            let locale = rust_i18n::locale();
                            let report = scan_directory(
                                path_str,
                                &skill_id,
                                &locale,
                                ScanOptions {
                                    skip_readme: false,
                                    ..Default::default()
                                },
                            )?;

                            log::info!(
                                "Scanned local skill '{}': score={}, files={:?}",
                                skill_name,
                                report.score,
                                report.scanned_files
                            );

                            // 创建 skill 对象（使用之前解析的元数据）
                            let local_path_str = path.to_string_lossy().to_string();
                            let linked_tools = scan_dir_tool.iter().cloned().collect::<Vec<_>>();
                            let skill = Skill {
                                id: skill_id,
                                name: skill_name,
                                description: skill_description,
                                repository_url: LOCAL_REPOSITORY_URL.to_string(),
                                repository_owner: Some(LOCAL_REPOSITORY_URL.to_string()),
                                file_path: path.to_string_lossy().to_string(),
                                version: None,
                                author: None,
                                installed: true,
                                installed_at: Some(Utc::now()),
                                local_path: Some(local_path_str.clone()),
                                local_paths: Some(vec![local_path_str.clone()]),
                                checksum: Some(checksum),
                                security_score: Some(report.score),
                                security_issues: Some(report.issues.clone()),
                                security_level: Some(report.level.as_str().to_string()),
                                security_report: Some(report.clone()),
                                scanned_at: Some(Utc::now()),
                                installed_commit_sha: None,
                                source_path: Some(local_path_str),
                                linked_tools,
                                is_local_only: true,
                                staging_path: None,
                            };

                            // 保存到数据库
                            self.db.save_skill(&skill)?;
                            imported_skills.push(skill.clone());
                            scanned_skills.push(skill);

                            log::info!("Imported local skill: {:?}", path);
                        }
                        Err(e) => {
                            log::warn!("Failed to read skill file {:?}: {}", skill_md_path, e);
                        }
                    }
                }
            }
        }

        // Reconciliation pass: detect skills that exist in DB as installed
        // but whose directories no longer exist on disk
        let stale_count = self.reconcile_stale_skills(&existing_skills)?;
        let refreshed_count = self.refresh_installed_tool_links()?;

        log::info!(
            "Scanned {} local skills, imported {} new skills, reconciled {} stale skills, refreshed {} tool-link states",
            scanned_skills.len(),
            imported_skills.len(),
            stale_count,
            refreshed_count
        );
        let reports = super::skill_reports::refresh_cross_skill_reports(&self.db)?;
        for skill in &mut scanned_skills {
            if let Some(report) = reports.get(&skill.id) {
                Self::apply_scan_report(skill, report);
            }
        }
        Ok(scanned_skills)
    }

    /// Reconcile DB state with filesystem: detect installed skills whose directories
    /// have been deleted externally and update DB accordingly.
    ///
    /// - For local-only skills with no remaining paths: delete the DB record entirely
    /// - For marketplace skills with no remaining paths: mark as uninstalled
    /// - For skills with some paths gone: prune stale paths and update linked_tools
    fn reconcile_stale_skills(&self, existing_skills: &[Skill]) -> Result<usize> {
        let mut stale_count = 0usize;

        for skill in existing_skills {
            if !skill.installed {
                continue;
            }

            let paths = match &skill.local_paths {
                Some(p) if !p.is_empty() => p.clone(),
                _ => match &skill.local_path {
                    Some(p) => vec![p.clone()],
                    None => continue,
                },
            };

            // Check which paths still exist on disk
            let alive_paths: Vec<String> = paths
                .iter()
                .filter(|p| PathBuf::from(p).exists())
                .cloned()
                .collect();

            if alive_paths.len() == paths.len() {
                continue; // All paths still exist, nothing to do
            }

            stale_count += 1;

            if alive_paths.is_empty() {
                // All paths are gone
                if skill.is_local_only {
                    log::info!(
                        "Deleting local-only skill '{}' (id={}): all paths removed from disk",
                        skill.name,
                        skill.id
                    );
                    if let Err(e) = self.db.delete_skill(&skill.id) {
                        log::warn!("Failed to delete stale local skill '{}': {}", skill.name, e);
                    }
                } else {
                    log::info!(
                        "Marking skill '{}' (id={}) as uninstalled: all paths removed from disk",
                        skill.name,
                        skill.id
                    );
                    let mut updated = skill.clone();
                    updated.installed = false;
                    updated.installed_at = None;
                    updated.local_path = None;
                    updated.local_paths = None;
                    updated.source_path = None;
                    updated.linked_tools = Vec::new();
                    if let Err(e) = self.db.save_skill(&updated) {
                        log::warn!("Failed to update stale skill '{}': {}", skill.name, e);
                    }
                }
            } else {
                // Some paths are gone — prune stale paths and clean up linked tools
                log::info!(
                    "Pruning stale paths for skill '{}' (id={}): {:?} -> {:?}",
                    skill.name,
                    skill.id,
                    paths,
                    alive_paths
                );

                let mut updated = skill.clone();

                // Remove linked tools whose paths are gone
                let stale_paths: std::collections::HashSet<&str> = paths
                    .iter()
                    .filter(|p| !alive_paths.contains(p))
                    .map(|s| s.as_str())
                    .collect();
                updated.linked_tools.retain(|tool_id| {
                    if let Some(tool) = AgentTool::from_id(tool_id) {
                        if let Some(tool_dir) = tool.default_skills_dir() {
                            // Check if any stale path belongs to this tool
                            let is_stale = stale_paths.iter().any(|sp| {
                                path_is_inside_dir_resolving_links(&PathBuf::from(sp), &tool_dir)
                            });
                            if is_stale {
                                return false;
                            }
                            // Also check: if any local_path was under this tool's dir and is now stale
                            let skill_path_was_in_tool_dir = updated
                                .local_paths
                                .as_ref()
                                .map(|paths| {
                                    paths.iter().any(|p| {
                                        let p_buf = PathBuf::from(p);
                                        path_is_inside_dir_resolving_links(&p_buf, &tool_dir)
                                    })
                                })
                                .unwrap_or(false);
                            let any_stale_in_tool_dir = stale_paths.iter().any(|sp| {
                                let sp_buf = PathBuf::from(sp);
                                normalize_path_for_compare(&sp_buf)
                                    .starts_with(&normalize_path_for_compare(&tool_dir))
                            });
                            if skill_path_was_in_tool_dir && any_stale_in_tool_dir {
                                return false;
                            }
                        }
                    }
                    true
                });

                updated.local_paths = Some(alive_paths.clone());
                updated.local_path = Some(alive_paths[0].clone());
                if updated
                    .source_path
                    .as_ref()
                    .map_or(true, |sp| !PathBuf::from(sp).exists())
                {
                    // 保留原 source_path 不自动迁移，标记为需要用户同步
                    log::warn!(
                        "技能 '{}' 的 source_path 不存在，保留原值。alive_paths: {:?}",
                        skill.name,
                        alive_paths
                    );
                }

                if let Err(e) = self.db.save_skill(&updated) {
                    log::warn!("Failed to prune paths for skill '{}': {}", skill.name, e);
                }
            }
        }

        Ok(stale_count)
    }

    /// 解析 SKILL.md 的 frontmatter（使用 serde_yaml，支持多行 block scalar 等 YAML 语法）
    fn parse_frontmatter(&self, content: &str) -> Result<(String, Option<String>)> {
        self.github.parse_skill_frontmatter(content)
    }

    /// 检测本地文件是否被修改（与缓存中的版本比较）
    #[cfg(test)]
    fn detect_local_modifications(
        &self,
        installed_dir: &PathBuf,
        cached_dir: &PathBuf,
    ) -> Result<Vec<String>> {
        Ok(super::content_manifest::changes(
            &super::content_manifest::manifest(cached_dir)?,
            &super::content_manifest::manifest(installed_dir)?,
        ))
    }

    fn installed_modifications(&self, skill: &Skill) -> Result<Vec<String>> {
        let Some(path) = skill.local_path.as_deref() else {
            return Ok(vec![
                "Installed content is unavailable; changes cannot be verified.".into(),
            ]);
        };
        let Some(baseline) = self.db.get_skill_baseline(&skill.id)? else {
            return Ok(vec![
                "Installed-version baseline is unavailable; changes cannot be verified.".into(),
            ]);
        };
        match super::content_manifest::manifest(Path::new(path)) {
            Ok(current) => Ok(super::content_manifest::changes(&baseline, &current)),
            Err(_) => Ok(vec![
                "Installed content could not be fully read; changes cannot be verified.".into(),
            ]),
        }
    }

    /// 准备技能更新：下载最新版本到临时目录并扫描，检测本地修改
    pub async fn prepare_skill_update(
        &self,
        skill_id: &str,
        locale: &str,
    ) -> Result<(crate::models::security::SecurityReport, Vec<String>)> {
        use anyhow::Context;

        log::info!("Preparing update for skill: {}", skill_id);

        // 获取技能信息
        let skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        if !skill.installed {
            anyhow::bail!("SKILL_NOT_INSTALLED");
        }

        // 获取仓库记录
        let repositories = self.db.get_repositories()?;
        let _repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("REPOSITORY_NOT_FOUND")?
            .clone();

        // 重新下载仓库到新的临时缓存（staging）
        log::info!("下载最新版本到 staging 目录");
        let (owner, repo_name) = crate::models::Repository::from_github_url(&skill.repository_url)?;

        let staging_base_dir = dirs::cache_dir()
            .context("CACHE_DIR_UNAVAILABLE")?
            .join("agent-skills-guard")
            .join("staging");

        std::fs::create_dir_all(&staging_base_dir)?;
        let staging_session = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(&staging_base_dir)?;
        std::fs::write(staging_session.path().join(UPDATE_SESSION_OWNER), &skill.id)?;
        // 下载最新版本
        let (extract_dir, new_commit_sha) = self
            .github
            .download_repository_archive(&owner, &repo_name, staging_session.path())
            .await
            .context("下载最新版本失败")?;

        log::info!("下载完成，最新 commit: {}", new_commit_sha);

        // 定位 staging 中的技能目录
        let staging_skill_dir =
            self.locate_skill_in_cache(extract_dir.as_path(), &skill.file_path)?;

        // 扫描最新版本
        let scan_report = scan_directory(
            staging_skill_dir
                .to_str()
                .context("INVALID_SKILL_DIR_PATH")?,
            &skill.id,
            locale,
            ScanOptions {
                skip_readme: false,
                ..Default::default()
            },
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        let mut modified_files = self.installed_modifications(&skill)?;
        if locale.starts_with("zh") {
            for item in &mut modified_files {
                *item = item
                    .replace("Deleted: ", "删除: ")
                    .replace("Modified: ", "修改: ")
                    .replace("Added: ", "新增: ")
                    .replace(
                        "Installed-version baseline is unavailable; changes cannot be verified.",
                        "缺少安装版本基线，无法确认本地修改。",
                    )
                    .replace(
                        "Installed content is unavailable; changes cannot be verified.",
                        "安装目录不可用，无法确认本地修改。",
                    )
                    .replace(
                        "Installed content could not be fully read; changes cannot be verified.",
                        "无法完整读取安装内容，无法确认本地修改。",
                    );
            }
        }
        let mut candidate = skill.clone();
        Self::apply_scan_report(&mut candidate, &scan_report);
        candidate.staging_path = Some(staging_skill_dir.to_string_lossy().to_string());
        candidate.installed_commit_sha = Some(
            self.resolve_path_aware_commit_sha(
                &skill.repository_url,
                &skill.file_path,
                &new_commit_sha,
            )
            .await,
        );
        let content = std::fs::read_to_string(staging_skill_dir.join("SKILL.md"))?;
        candidate.checksum = Some(calculate_checksum(content.as_bytes()));
        self.persist_update_preparation(&skill, &candidate, staging_session)?;

        Ok((scan_report, modified_files))
    }

    fn persist_update_preparation(
        &self,
        active: &Skill,
        candidate: &Skill,
        session: tempfile::TempDir,
    ) -> Result<()> {
        self.db.save_skill_preparation(candidate)?;
        let mut updated = active.clone();
        updated.staging_path = candidate.staging_path.clone();
        self.db.save_skill(&updated)?;
        let _ = session.keep();
        if let Some(old) = resolve_staging_dir(active) {
            if active.staging_path != candidate.staging_path {
                if let Err(error) = cleanup_update_staging(&old, &active.id) {
                    log::warn!("新更新已准备，旧暂存清理失败: {error}");
                }
            }
        }
        Ok(())
    }

    /// 确认技能更新：从 staging 写入到安装目录，并在缓存目录保留备份
    pub fn confirm_skill_update(
        &self,
        skill_id: &str,
        force_overwrite: bool,
        allow_partial_scan: bool,
    ) -> Result<()> {
        invalidate_skill_md_checksum_cache();
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;
        if !skill.installed {
            anyhow::bail!("SKILL_NOT_INSTALLED");
        }
        let staging = resolve_staging_dir(&skill).context("技能尚未准备更新")?;
        let staging = ensure_staging_path_is_contained(&staging)?;
        // No mutation of the active installation may precede the security gate.
        self.rescan_skill_directory_for_confirmation(&staging, &skill.id, allow_partial_scan)?;
        if !force_overwrite && !self.installed_modifications(&skill)?.is_empty() {
            anyhow::bail!("LOCAL_MODIFICATIONS: 请确认覆盖本地修改或无法核实的旧版本");
        }
        let raw = skill
            .local_path
            .as_deref()
            .filter(|p| !p.starts_with(STAGING_LOCAL_PATH_PREFIX))
            .map(PathBuf::from)
            .or_else(|| {
                skill
                    .local_paths
                    .as_ref()
                    .and_then(|p| p.last())
                    .map(PathBuf::from)
            })
            .context("SKILL_NO_INSTALL_PATH")?;
        let (display, target) = resolve_update_install_paths(&raw);
        let parent = target.parent().context("INVALID_INSTALL_PATH")?;
        std::fs::create_dir_all(parent)?;
        let name = target
            .file_name()
            .context("INVALID_SKILL_DIR_NAME")?
            .to_string_lossy();
        let prepared = self.create_temp_install_dir(parent, &name)?;
        self.copy_dir_recursive(&staging, prepared.path(), &mut 0)?;
        let report = self.rescan_skill_directory_for_confirmation(
            prepared.path(),
            &skill.id,
            allow_partial_scan,
        )?;
        let baseline = super::content_manifest::manifest(prepared.path())?;
        let candidate = self
            .db
            .get_skill_preparation(skill_id)?
            .filter(|c| c.staging_path == skill.staging_path);
        let identity = hash_bytes(skill.id.as_bytes());
        let batch = uuid::Uuid::new_v4().to_string();
        let backup = parent.join(format!(".{name}.backup-{}-{batch}", &identity[..16]));
        let had_old = target.exists();
        if had_old {
            rename_with_retry(&target, &backup).context("无法备份旧技能目录")?;
        }
        let result = (|| -> Result<()> {
            rename_with_retry(prepared.path(), &target).context("无法写入新技能目录")?;
            skill.local_path = Some(display.to_string_lossy().to_string());
            skill.staging_path = None;
            skill.installed_at = Some(Utc::now());
            if let Some(candidate) = candidate {
                skill.installed_commit_sha = candidate.installed_commit_sha;
                skill.checksum = candidate.checksum;
            }
            Self::apply_scan_report(&mut skill, &report);
            self.db
                .save_installed_skill_with_baseline(&skill, &baseline)?;
            Ok(())
        })();
        if let Err(error) = result {
            if had_old {
                restore_installation_backup(&backup, &target).with_context(|| {
                    format!(
                        "更新失败: {error}; 回滚失败，备份保留在 {}",
                        backup.display()
                    )
                })?;
            } else if target.exists() {
                std::fs::remove_dir_all(&target).context("更新失败且无法清理新目录")?;
            }
            return Err(error);
        }
        // Keep an identity- and batch-specific recovery copy. If the cache is on another
        // volume, preserve the same-volume backup instead of risking a lossy copy.
        if had_old {
            if let Some(cache) = dirs::cache_dir() {
                let root = cache
                    .join("agent-skills-guard/skill-backups")
                    .join(&identity);
                if std::fs::create_dir_all(&root).is_err()
                    || std::fs::rename(&backup, root.join(&batch)).is_err()
                {
                    log::warn!("备份保留在 {}", backup.display());
                }
            }
        }
        if let Err(error) = cleanup_update_staging(&staging, &skill.id) {
            log::warn!("更新已完成，暂存清理失败: {error}");
        }
        self.invalidate_installed_cache();
        if let Err(error) = super::skill_reports::refresh_cross_skill_reports(&self.db) {
            log::warn!("更新已完成，关联报告刷新失败: {error}");
        }
        Ok(())
    }

    /// 取消技能更新：清理 staging 目录
    pub fn cancel_skill_update(&self, skill_id: &str) -> Result<()> {
        use anyhow::Context;

        log::info!("Canceling update for skill: {}", skill_id);

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        let Some(staging_dir) = resolve_staging_dir(&skill) else {
            self.db.clear_skill_preparation(skill_id)?;
            log::warn!("技能没有处于更新准备状态");
            return Ok(());
        };

        cleanup_update_staging(&staging_dir, skill_id)?;

        // 新流程下 local_path 已是真实安装路径；旧 __staging__: 记录才需要恢复。
        let is_legacy_staging = skill
            .local_path
            .as_deref()
            .and_then(legacy_staging_path)
            .is_some();
        if is_legacy_staging {
            skill.local_path = skill
                .local_paths
                .as_ref()
                .and_then(|paths| paths.last().cloned());
        }
        skill.staging_path = None;
        self.db.clear_skill_preparation(skill_id)?;

        self.db.save_skill(&skill)?;

        log::info!("技能更新已取消: {}", skill.name);
        Ok(())
    }

    /// 为指定 skill 重建到目标工具的链接
    /// - 本地技能先自动提升到通用目录（~/.agents/skills），原位置替换为链接
    /// - 再按当前 linked_tools 删除旧链接
    /// - 最后为 target_tools（不含 agents）创建新链接
    pub fn sync_skill_to_tools(&self, skill_id: &str, target_tools: Vec<String>) -> Result<()> {
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("SKILL_NOT_FOUND")?;

        // 本地技能自动提升：复制到通用目录，原位置替换为 Junction
        if skill.is_local_only {
            let original_path = skill
                .local_path
                .as_ref()
                .context("本地技能缺少 local_path")?
                .clone();
            let original = PathBuf::from(&original_path);
            let dir_name = original.file_name().context("INVALID_SKILL_DIR_NAME")?;
            let common_dir = self.skills_dir.join(&dir_name);

            // 确保通用目录的父目录存在
            std::fs::create_dir_all(&self.skills_dir).context("无法创建通用技能目录")?;

            let already_in_common_dir = paths_point_to_same_location(&original, &common_dir);

            if already_in_common_dir {
                log::info!("本地技能已位于通用目录，无需复制: {:?}", common_dir);
            } else {
                if std::fs::symlink_metadata(&common_dir).is_ok() {
                    anyhow::bail!(
                        "INSTALL_TARGET_EXISTS: 同名通用技能目录已存在: {}",
                        common_dir.display()
                    );
                }
                let temp =
                    self.create_temp_install_dir(&self.skills_dir, &dir_name.to_string_lossy())?;
                let mut files_copied = 0;
                let original_identity = super::content_manifest::manifest(&original)?;
                self.copy_dir_recursive(&original, temp.path(), &mut files_copied)?;
                if super::content_manifest::manifest(temp.path())? != original_identity
                    || super::content_manifest::manifest(&original)? != original_identity
                {
                    anyhow::bail!(
                        "SYNC_SOURCE_CHANGED: source changed or could not be copied completely"
                    );
                }
                rename_with_retry(temp.path(), &common_dir)?;
                log::info!(
                    "已将本地技能复制到通用目录: {:?} ({} 个文件)",
                    common_dir,
                    files_copied
                );

                let original_backup = original.with_file_name(format!(
                    ".{}.sync-backup-{}",
                    dir_name.to_string_lossy(),
                    uuid::Uuid::new_v4()
                ));
                if let Err(error) = rename_with_retry(&original, &original_backup) {
                    let _ = std::fs::remove_dir_all(&common_dir);
                    return Err(error).context("无法备份原技能目录");
                }
                if let Err(error) = link_fs::create_dir_link(&common_dir, &original) {
                    restore_installation_backup(&original_backup, &original).with_context(
                        || {
                            format!(
                                "同步失败: {error}; 恢复失败，备份在 {}",
                                original_backup.display()
                            )
                        },
                    )?;
                    let _ = std::fs::remove_dir_all(&common_dir);
                    return Err(error).context("无法将原位置替换为链接，原内容已恢复");
                }
                let cleanup = if link_fs::is_dir_link(&original_backup) {
                    link_fs::remove_dir_link(&original_backup)
                } else {
                    std::fs::remove_dir_all(&original_backup).map_err(Into::into)
                };
                if let Err(error) = cleanup {
                    log::warn!("同步成功，备份清理失败: {error}");
                }
            }

            // 更新技能记录
            let common_str = common_dir.to_string_lossy().to_string();
            skill.source_path = Some(common_str.clone());
            skill.local_path = Some(common_str.clone());
            skill.local_paths = if already_in_common_dir {
                Some(vec![common_str])
            } else {
                Some(vec![common_str, original_path.clone()])
            };
            skill.is_local_only = false;

            // 识别原路径所属工具并加入 linked_tools
            let mut linked = Vec::new();
            let original = PathBuf::from(&original_path);
            for tool in AgentTool::all() {
                if tool == AgentTool::Agents {
                    continue;
                }
                if let Some(tool_dir) = tool.default_skills_dir() {
                    if path_is_inside_dir_resolving_links(&original, &tool_dir) {
                        linked.push(tool.id().to_string());
                        break;
                    }
                }
            }
            skill.linked_tools = linked;
            self.db.save_skill(&skill)?;
        }

        let source_path = skill
            .source_path
            .as_ref()
            .or(skill.local_path.as_ref())
            .context("技能未安装（无 source_path）")?
            .clone();
        let source = PathBuf::from(&source_path);

        let skill_dir_name = source
            .file_name()
            .context("INVALID_SKILL_DIR_NAME")?
            .to_string_lossy()
            .to_string();

        // 先创建新链接，再删除不再需要的旧链接（原子性：新链接创建失败不影响已有链接）
        let mut linked_tool_ids: Vec<String> = Vec::new();
        let mut link_failures: Vec<ToolLinkFailure> = Vec::new();
        for tool_id in &target_tools {
            let tool = match AgentTool::from_id(tool_id) {
                Some(t) if t != AgentTool::Agents => t,
                _ => continue,
            };
            if let Some(tool_dir) = tool.default_skills_dir() {
                let link = tool_dir.join(&skill_dir_name);
                create_or_reuse_tool_link(
                    &source,
                    &link,
                    tool.id(),
                    skill.checksum.as_deref(),
                    &mut linked_tool_ids,
                    &mut link_failures,
                );
            }
        }

        // 删除旧链接（仅删除不在新 target_tools 中且确实指向自身源的链接）
        for old_tool_id in &skill.linked_tools {
            if target_tools.iter().any(|t| t == old_tool_id) {
                continue; // 仍在目标工具列表中，保留
            }
            if let Some(tool) = AgentTool::from_id(old_tool_id) {
                if let Some(tool_dir) = tool.default_skills_dir() {
                    let link = tool_dir.join(&skill_dir_name);
                    if link_fs::is_dir_link(&link) {
                        let targets_source = link_fs::read_dir_link_target(&link)
                            .map(|target| paths_point_to_same_location(&source, &target))
                            .unwrap_or(false);
                        if targets_source {
                            if let Err(e) = link_fs::remove_dir_link(&link) {
                                log::warn!("删除旧链接失败 [{:?}]: {}", link, e);
                            }
                        } else {
                            log::warn!(
                                "跳过删除旧链接 [{:?}]：链接目标不指向源目录，可能为第三方工具建立",
                                link
                            );
                        }
                    }
                }
            }
        }

        let reconciled = refresh_existing_tool_links_for_skill(&skill, &default_tool_dirs());
        self.db.save_skill(&reconciled)?;

        log::info!("Synced skill '{}' to tools: {:?}", skill.name, target_tools);
        self.invalidate_installed_cache();
        if link_failures.is_empty() {
            Ok(())
        } else {
            anyhow::bail!(
                "SYNC_PARTIAL_FAILURE: {}",
                format_tool_link_failures(&link_failures)
            )
        }
    }

    /// 为所有已安装 skill 批量同步到指定工具（含本地技能）
    pub fn sync_all_skills_to_tools(&self, target_tools: Vec<String>) -> Result<()> {
        let skills = self.db.get_skills()?;
        let managed: Vec<_> = skills.into_iter().filter(|s| s.installed).collect();

        let mut errors = Vec::new();
        for skill in managed {
            if let Err(e) = self.sync_skill_to_tools(&skill.id, target_tools.clone()) {
                log::warn!("同步 skill '{}' 失败: {}", skill.name, e);
                errors.push(format!("{}: {}", skill.name, e));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            anyhow::bail!("SYNC_BATCH_PARTIAL_FAILURE:\n{}", errors.join("\n"))
        }
    }
}

fn is_retryable_rename_error(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }

    matches!(err.raw_os_error(), Some(5 | 32 | 33))
}

pub(crate) fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last_err: Option<std::io::Error> = None;
    let attempts = 8usize;

    for attempt in 0..attempts {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(err) => {
                let retryable = is_retryable_rename_error(&err);
                let is_last = attempt + 1 >= attempts;
                last_err = Some(err);
                if retryable && !is_last {
                    // 指数退避（上限 4s）：250ms, 500ms, 1000ms, 2000ms, 4000ms, 4000ms, 4000ms
                    // Windows 病毒扫描可能持有句柄 > 2 秒
                    let delay_ms = (250u64.saturating_mul(1u64 << attempt)).min(4000);
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    continue;
                }
                break;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::other("rename_with_retry failed")))
}

#[cfg(test)]
mod tests {
    use super::{
        build_local_skill_id, build_synced_tool_state, create_or_reuse_tool_link,
        ensure_custom_install_target_is_owned, ensure_update_clear_has_backup,
        find_tool_id_for_scan_dir, install_base_conflicting_tool,
        invalidate_skill_md_checksum_cache, paths_point_to_same_location,
        refresh_existing_tool_links_for_skill, resolve_update_install_paths,
        resolve_update_target_install_dir, restore_installation_backup, skill_md_checksum,
        skill_md_checksum_with_cache, tool_skill_path_is_compatible_with_source, ToolSkillDir,
        STAGING_LOCAL_PATH_PREFIX,
    };
    use crate::models::Skill;
    use crate::services::agent_tools::AgentTool;
    use crate::services::{link_fs, Database};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn custom_install_does_not_replace_untracked_existing_directory() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("authorized_keys");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "unrelated").unwrap();
        let skill = Skill::new(
            "example".to_string(),
            "https://github.com/example/repo".to_string(),
            "skills/authorized_keys".to_string(),
        );

        let error = ensure_custom_install_target_is_owned(&skill, &target, true)
            .expect_err("untracked target must not be replaced");
        assert!(error.to_string().contains("INSTALL_TARGET_EXISTS"));
        assert_eq!(
            std::fs::read_to_string(target.join("keep.txt")).unwrap(),
            "unrelated"
        );
    }

    #[test]
    fn custom_reinstall_allows_exact_tracked_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("example");
        std::fs::create_dir(&target).unwrap();
        let mut skill = Skill::new(
            "example".to_string(),
            "https://github.com/example/repo".to_string(),
            "skills/example".to_string(),
        );
        skill.local_path = Some(target.to_string_lossy().into_owned());

        ensure_custom_install_target_is_owned(&skill, &target, true).unwrap();
    }

    #[test]
    fn forced_update_never_clears_existing_target_without_backup() {
        let error = ensure_update_clear_has_backup(true, true, false)
            .expect_err("destructive clear without backup must be rejected");
        assert!(error.to_string().contains("UPDATE_BACKUP_REQUIRED"));

        ensure_update_clear_has_backup(true, true, true).unwrap();
        ensure_update_clear_has_backup(false, true, false).unwrap();
    }

    #[test]
    fn build_synced_tool_state_replaces_previous_links_with_requested_targets() {
        let source = PathBuf::from("/tmp/.agents/skills/example");
        let target_tools = vec!["codex".to_string()];

        let (linked_tools, local_paths) =
            build_synced_tool_state(&source, "example", &target_tools);

        assert_eq!(linked_tools, vec!["codex".to_string()]);
        assert_eq!(local_paths[0], "/tmp/.agents/skills/example");
        assert!(local_paths
            .iter()
            .map(|path| path.replace('\\', "/"))
            .any(|path| path.ends_with(".codex/skills/example")));
        assert!(!linked_tools.contains(&"claude-code".to_string()));
    }

    #[test]
    fn build_synced_tool_state_clears_links_when_no_targets_are_requested() {
        let source = PathBuf::from("/tmp/.agents/skills/example");

        let (linked_tools, local_paths) = build_synced_tool_state(&source, "example", &[]);

        assert!(linked_tools.is_empty());
        assert_eq!(local_paths, vec!["/tmp/.agents/skills/example".to_string()]);
    }

    #[test]
    fn tool_link_failures_are_formatted_with_tool_ids_and_reasons() {
        let failures = vec![super::ToolLinkFailure::new(
            "codex",
            "无法创建符号链接: permission denied",
        )];

        assert_eq!(
            super::format_tool_link_failures(&failures),
            "codex (无法创建符号链接: permission denied)"
        );
    }

    #[test]
    fn successful_directory_link_counts_as_linked_tool_even_when_it_resolves_to_source() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join(".agents").join("skills").join("example");
        let link = temp.path().join(".codex").join("skills").join("example");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "test").unwrap();

        let mut linked_tool_ids = Vec::new();
        let mut link_failures = Vec::new();
        create_or_reuse_tool_link(
            &source,
            &link,
            "codex",
            None,
            &mut linked_tool_ids,
            &mut link_failures,
        );

        assert!(link.exists());
        assert!(
            paths_point_to_same_location(&source, &link),
            "created tool link should resolve to the source directory"
        );
        assert_eq!(linked_tool_ids, vec!["codex".to_string()]);
        assert!(link_failures.is_empty());
    }

    #[test]
    fn install_base_conflict_allows_unified_agents_dir() {
        let agents_dir = AgentTool::Agents.default_skills_dir().unwrap();

        assert_eq!(install_base_conflicting_tool(&agents_dir), None);
    }

    #[test]
    fn install_base_conflict_rejects_non_agents_tool_dirs() {
        let codex_dir = AgentTool::Codex.default_skills_dir().unwrap();
        let codex_child = codex_dir.join("nested");

        assert_eq!(
            install_base_conflicting_tool(&codex_dir),
            Some(AgentTool::Codex)
        );
        assert_eq!(
            install_base_conflicting_tool(&codex_child),
            Some(AgentTool::Codex)
        );
    }

    #[test]
    fn local_skill_id_includes_path_not_only_checksum() {
        let checksum = "b81e2ff87ed8fa4d0b81e2ff87ed8fa4d";
        let agents_path = PathBuf::from("C:/Users/Bruce/.agents/skills/frontend-design");
        let claude_path = PathBuf::from("C:/Users/Bruce/.claude/skills/frontend-design");

        assert_ne!(
            build_local_skill_id(checksum, &agents_path),
            build_local_skill_id(checksum, &claude_path)
        );
    }

    #[test]
    fn same_path_detection_handles_already_promoted_local_skill() {
        let original = PathBuf::from("C:/Users/Bruce/.agents/skills/frontend-design");
        let common = PathBuf::from("C:/Users/Bruce/.agents/skills/frontend-design");

        assert!(paths_point_to_same_location(&original, &common));
    }

    #[test]
    fn update_target_resolution_follows_directory_links() {
        let temp = tempfile::tempdir().unwrap();
        let real_dir = temp.path().join("real-skill");
        let link_dir = temp.path().join("linked-skill");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::fs::write(real_dir.join("SKILL.md"), "test").unwrap();
        link_fs::create_dir_link(&real_dir, &link_dir).unwrap();

        let resolved = resolve_update_target_install_dir(&link_dir);

        assert!(
            paths_point_to_same_location(&resolved, &real_dir),
            "updates must write through directory links so every linked tool sees the new files"
        );
    }

    #[test]
    fn update_install_paths_preserve_display_path_for_links() {
        let temp = tempfile::tempdir().unwrap();
        let real_dir = temp.path().join("real-skill");
        let link_dir = temp.path().join("linked-skill");
        std::fs::create_dir_all(&real_dir).unwrap();
        link_fs::create_dir_link(&real_dir, &link_dir).unwrap();

        let (display_path, write_target) = resolve_update_install_paths(&link_dir);

        assert!(paths_point_to_same_location(&display_path, &link_dir));
        assert!(paths_point_to_same_location(&write_target, &real_dir));
    }

    #[test]
    fn scan_dir_tool_detection_follows_linked_tool_directory_targets() {
        let temp = tempfile::tempdir().unwrap();
        let real_tools_dir = temp.path().join("real-claude-skills");
        let linked_tools_dir = temp.path().join("home").join(".claude").join("skills");
        std::fs::create_dir_all(&real_tools_dir).unwrap();
        link_fs::create_dir_link(&real_tools_dir, &linked_tools_dir).unwrap();

        let mut dir_to_tool = HashMap::new();
        dir_to_tool.insert(linked_tools_dir, "claude-code".to_string());

        assert_eq!(
            find_tool_id_for_scan_dir(&real_tools_dir, &dir_to_tool).as_deref(),
            Some("claude-code")
        );
    }

    #[test]
    fn installed_skill_state_detects_existing_tool_directory_links() {
        let temp = tempfile::tempdir().unwrap();
        let agents_dir = temp.path().join(".agents").join("skills");
        let claude_dir = temp.path().join(".claude").join("skills");
        let source = agents_dir.join("frontend-design");
        let claude_link = claude_dir.join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "test").unwrap();
        link_fs::create_dir_link(&source, &claude_link).unwrap();

        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = true;
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.linked_tools = Vec::new();

        let tool_dirs = vec![
            (agents_dir, "agents".to_string()),
            (claude_dir, "claude-code".to_string()),
        ];

        let refreshed = refresh_existing_tool_links_for_skill(&skill, &tool_dirs);

        assert!(refreshed.linked_tools.contains(&"claude-code".to_string()));
        assert!(refreshed
            .local_paths
            .as_ref()
            .unwrap()
            .iter()
            .any(|path| path
                .replace('\\', "/")
                .ends_with(".claude/skills/frontend-design")));
    }

    #[test]
    fn installed_skill_state_prunes_stale_agents_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = codex_dir.join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "test").unwrap();

        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = true;
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.linked_tools = vec!["agents".to_string(), "codex".to_string()];

        let refreshed =
            refresh_existing_tool_links_for_skill(&skill, &[(codex_dir, "codex".to_string())]);

        assert!(!refreshed.linked_tools.contains(&"agents".to_string()));
        assert!(refreshed.linked_tools.contains(&"codex".to_string()));
    }

    #[test]
    fn installed_skill_state_adopts_existing_tool_directory_with_same_checksum() {
        let temp = tempfile::tempdir().unwrap();
        let agents_dir = temp.path().join(".agents").join("skills");
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = agents_dir.join("frontend-design");
        let codex_existing = codex_dir.join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&codex_existing).unwrap();
        std::fs::write(source.join("SKILL.md"), "same skill").unwrap();
        std::fs::write(codex_existing.join("SKILL.md"), "same skill").unwrap();

        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(source.to_string_lossy().to_string());
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.checksum = Some(crate::security::calculate_checksum(b"same skill"));
        skill.linked_tools = Vec::new();

        let refreshed = refresh_existing_tool_links_for_skill(
            &skill,
            &[(codex_dir.clone(), "codex".to_string())],
        );

        assert!(refreshed.linked_tools.contains(&"codex".to_string()));
        assert!(refreshed
            .local_paths
            .as_ref()
            .unwrap()
            .contains(&codex_existing.to_string_lossy().to_string()));
    }

    #[test]
    fn installed_skill_state_drops_missing_agents_source_when_compatible_tool_path_exists() {
        let temp = tempfile::tempdir().unwrap();
        let missing_agents_source = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("frontend-design");
        let codex_dir = temp.path().join(".codex").join("skills");
        let codex_existing = codex_dir.join("frontend-design");
        std::fs::create_dir_all(&codex_existing).unwrap();
        std::fs::write(codex_existing.join("SKILL.md"), "same skill").unwrap();

        let checksum = crate::security::calculate_checksum(b"same skill");
        let mut skill = crate::models::Skill::new(
            "frontend-design".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            missing_agents_source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(missing_agents_source.to_string_lossy().to_string());
        skill.local_path = Some(missing_agents_source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![missing_agents_source.to_string_lossy().to_string()]);
        skill.checksum = Some(checksum);
        skill.linked_tools = Vec::new();

        let refreshed =
            refresh_existing_tool_links_for_skill(&skill, &[(codex_dir, "codex".to_string())]);

        let codex_path = codex_existing.to_string_lossy().to_string();
        assert_eq!(refreshed.source_path.as_deref(), Some(codex_path.as_str()));
        assert_eq!(refreshed.local_path.as_deref(), Some(codex_path.as_str()));
        assert_eq!(refreshed.local_paths.as_deref(), Some(&[codex_path][..]));
        assert!(refreshed.linked_tools.contains(&"codex".to_string()));
    }

    #[test]
    fn manual_scan_adopts_existing_tool_links_before_scanning() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let claude_dir = temp.path().join(".claude").join("skills");
        let agents_skill = agents_dir.join("frontend-design");
        let claude_skill = claude_dir.join("frontend-design");
        std::fs::create_dir_all(&agents_skill).unwrap();
        std::fs::write(
            agents_skill.join("SKILL.md"),
            "---\nname: frontend-design\ndescription: Shared skill\n---\n",
        )
        .unwrap();
        link_fs::create_dir_link(&agents_skill, &claude_skill).unwrap();

        manager
            .adopt_existing_skills_for_scan(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir.clone(),
                },
                ToolSkillDir {
                    tool_id: "claude-code".to_string(),
                    path: claude_dir.clone(),
                },
            ])
            .unwrap();

        let skills = db.get_skills().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(
            skills[0].source_path.as_deref(),
            Some(agents_skill.to_string_lossy().as_ref())
        );
        assert!(skills[0]
            .local_paths
            .as_ref()
            .unwrap()
            .contains(&claude_skill.to_string_lossy().to_string()));
        assert_eq!(skills[0].linked_tools, vec!["claude-code".to_string()]);
    }

    #[test]
    fn refresh_skill_links_detects_newly_created_tool_symlink_without_restart() {
        // 模拟应用内更新后：技能已安装且 linked_tools 为空（图标未点亮），
        // 此时磁盘上已存在 .claude/skills/example 软链。
        // refresh_skill_links 必须能在不重启进程的情况下点亮 linked_tools。
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let claude_dir = temp.path().join(".claude").join("skills");
        let source = agents_dir.join("example");
        let claude_link = claude_dir.join("example");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: example\n---\n").unwrap();
        link_fs::create_dir_link(&source, &claude_link).unwrap();

        // 写入一条已安装、但 linked_tools 尚为空的记录
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = true;
        skill.source_path = Some(source.to_string_lossy().to_string());
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        skill.linked_tools = Vec::new();
        db.save_skill(&skill).unwrap();

        let tool_dirs = vec![
            (agents_dir, "agents".to_string()),
            (claude_dir, "claude-code".to_string()),
        ];

        let installed = manager.refresh_skill_links_from_dirs(&tool_dirs).unwrap();

        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].linked_tools, vec!["claude-code".to_string()]);
        // DB 也应被刷新
        let from_db = db.get_skills().unwrap();
        assert_eq!(from_db[0].linked_tools, vec!["claude-code".to_string()]);
    }

    #[test]
    fn installed_skills_prune_deleted_tool_link_without_restart() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = agents_dir.join("example");
        let codex_link = codex_dir.join("example");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: example\n---\n").unwrap();
        link_fs::create_dir_link(&source, &codex_link).unwrap();

        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(source.to_string_lossy().to_string());
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![
            source.to_string_lossy().to_string(),
            codex_link.to_string_lossy().to_string(),
        ]);
        skill.linked_tools = vec!["codex".to_string()];
        db.save_skill(&skill).unwrap();

        let first = manager
            .get_installed_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir.clone(),
                },
                ToolSkillDir {
                    tool_id: "codex".to_string(),
                    path: codex_dir.clone(),
                },
            ])
            .unwrap();
        assert_eq!(first[0].linked_tools, vec!["codex".to_string()]);

        link_fs::remove_dir_link(&codex_link).unwrap();

        let refreshed = manager
            .get_installed_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir,
                },
                ToolSkillDir {
                    tool_id: "codex".to_string(),
                    path: codex_dir,
                },
            ])
            .unwrap();

        assert!(refreshed[0].linked_tools.is_empty());
        assert_eq!(
            refreshed[0].local_paths.as_deref(),
            Some(&[source.to_string_lossy().to_string()][..])
        );
    }

    #[test]
    fn installed_skills_detect_new_tool_link_without_restart() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = agents_dir.join("example");
        let codex_link = codex_dir.join("example");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: example\n---\n").unwrap();

        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(source.to_string_lossy().to_string());
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        db.save_skill(&skill).unwrap();

        let first = manager
            .get_installed_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir.clone(),
                },
                ToolSkillDir {
                    tool_id: "codex".to_string(),
                    path: codex_dir.clone(),
                },
            ])
            .unwrap();
        assert!(first[0].linked_tools.is_empty());

        link_fs::create_dir_link(&source, &codex_link).unwrap();

        let refreshed = manager
            .get_installed_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir,
                },
                ToolSkillDir {
                    tool_id: "codex".to_string(),
                    path: codex_dir,
                },
            ])
            .unwrap();

        assert_eq!(refreshed[0].linked_tools, vec!["codex".to_string()]);
        assert!(refreshed[0]
            .local_paths
            .as_ref()
            .unwrap()
            .contains(&codex_link.to_string_lossy().to_string()));
    }

    #[test]
    fn installed_skills_adopt_untracked_linked_skill_without_restart() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let codex_dir = temp.path().join(".codex").join("skills");
        let source = agents_dir.join("example");
        let codex_link = codex_dir.join("example");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: example\n---\n").unwrap();
        link_fs::create_dir_link(&source, &codex_link).unwrap();

        let installed = manager
            .get_installed_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir,
                },
                ToolSkillDir {
                    tool_id: "codex".to_string(),
                    path: codex_dir,
                },
            ])
            .unwrap();

        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].name, "example");
        assert_eq!(installed[0].linked_tools, vec!["codex".to_string()]);
        assert!(installed[0]
            .local_paths
            .as_ref()
            .unwrap()
            .contains(&codex_link.to_string_lossy().to_string()));
    }

    #[test]
    fn all_skills_refresh_detects_new_tool_link_without_restart() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let claude_dir = temp.path().join(".claude").join("skills");
        let source = agents_dir.join("example");
        let claude_link = claude_dir.join("example");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: example\n---\n").unwrap();

        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(source.to_string_lossy().to_string());
        skill.local_path = Some(source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![source.to_string_lossy().to_string()]);
        db.save_skill(&skill).unwrap();

        link_fs::create_dir_link(&source, &claude_link).unwrap();

        let refreshed = manager
            .get_all_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir,
                },
                ToolSkillDir {
                    tool_id: "claude-code".to_string(),
                    path: claude_dir,
                },
            ])
            .unwrap();

        assert_eq!(refreshed[0].linked_tools, vec!["claude-code".to_string()]);
        assert!(refreshed[0]
            .local_paths
            .as_ref()
            .unwrap()
            .contains(&claude_link.to_string_lossy().to_string()));
    }

    /// SKILL.md 校验和缓存：命中、写入与失效。
    ///
    /// `refresh_existing_tool_links_for_skill` 对每个技能 × 每个工具目录都要比对
    /// 一次校验和，缓存是消除 O(N×T) 次 SHA256 的关键。
    ///
    /// 使用独立缓存而非全局缓存：后者是进程级状态，`cargo test` 并行执行时
    /// 其他测试的安装 / 卸载路径会调用 `invalidate_skill_md_checksum_cache()`，
    /// 断言全局缓存必然出现偶发失败。
    #[test]
    fn skill_md_checksum_cache_hits_writes_and_invalidates() {
        let cache: super::ChecksumCache = std::sync::Mutex::new(HashMap::new());
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("example");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        std::fs::write(&skill_md, "original").unwrap();

        // 首次读取应把结果写入缓存
        let original = skill_md_checksum_with_cache(&skill_dir, &cache).expect("first read");
        assert!(
            cache.lock().unwrap().contains_key(&skill_md),
            "first read should populate the cache"
        );

        // 篡改缓存值：若第二次调用命中缓存，就会原样返回被篡改的值，
        // 从而证明它没有重新读盘 + 哈希。
        cache.lock().unwrap().get_mut(&skill_md).unwrap().checksum =
            "sentinel-cached-value".to_string();
        assert_eq!(
            skill_md_checksum_with_cache(&skill_dir, &cache).as_deref(),
            Some("sentinel-cached-value"),
            "unchanged file should be served from cache without re-hashing"
        );

        // 内容变化（长度不同）必须绕过缓存，重新计算
        std::fs::write(&skill_md, "changed content that differs in length").unwrap();
        let changed = skill_md_checksum_with_cache(&skill_dir, &cache).unwrap();
        assert_ne!(changed, "sentinel-cached-value");
        assert_ne!(changed, original);

        // 文件被删除时不得返回陈旧校验和
        std::fs::remove_file(&skill_md).unwrap();
        assert_eq!(
            skill_md_checksum_with_cache(&skill_dir, &cache),
            None,
            "a deleted SKILL.md must not report a stale checksum"
        );
    }

    /// 安装 / 更新 / 卸载会覆写技能目录，必须能显式清空全局缓存。
    #[test]
    fn invalidate_clears_global_checksum_cache() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join("example");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "content").unwrap();

        let checksum = skill_md_checksum(&skill_dir).expect("read via global cache");

        invalidate_skill_md_checksum_cache();

        // 失效后重新计算，结果应与内容一致（不依赖缓存是否残留）
        assert_eq!(
            skill_md_checksum(&skill_dir).as_deref(),
            Some(checksum.as_str()),
            "recomputed checksum must match the file content"
        );
    }

    #[test]
    fn installed_skills_reuse_existing_record_when_source_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let agents_dir = temp.path().join(".agents").join("skills");
        let missing_source = agents_dir.join("example");
        let codex_dir = temp.path().join(".codex").join("skills");
        let codex_existing = codex_dir.join("example");
        std::fs::create_dir_all(&codex_existing).unwrap();
        std::fs::write(codex_existing.join("SKILL.md"), "same skill").unwrap();

        let checksum = crate::security::calculate_checksum(b"same skill");
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            missing_source.to_string_lossy().to_string(),
        );
        skill.id = "skill-1".to_string();
        skill.installed = true;
        skill.is_local_only = false;
        skill.source_path = Some(missing_source.to_string_lossy().to_string());
        skill.local_path = Some(missing_source.to_string_lossy().to_string());
        skill.local_paths = Some(vec![missing_source.to_string_lossy().to_string()]);
        skill.checksum = Some(checksum);
        db.save_skill(&skill).unwrap();

        let installed = manager
            .get_installed_skills_from_dirs(&[
                ToolSkillDir {
                    tool_id: "agents".to_string(),
                    path: agents_dir,
                },
                ToolSkillDir {
                    tool_id: "codex".to_string(),
                    path: codex_dir,
                },
            ])
            .unwrap();

        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].id, "skill-1");
        assert_eq!(
            installed[0].source_path.as_deref(),
            Some(codex_existing.to_string_lossy().as_ref())
        );
        assert_eq!(installed[0].linked_tools, vec!["codex".to_string()]);
    }

    #[test]
    fn tool_skill_path_is_not_compatible_when_same_name_has_different_checksum() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("frontend-design");
        let target = temp
            .path()
            .join(".codex")
            .join("skills")
            .join("frontend-design");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(source.join("SKILL.md"), "source skill").unwrap();
        std::fs::write(target.join("SKILL.md"), "different skill").unwrap();

        let source_checksum = crate::security::calculate_checksum(b"source skill");

        assert!(!tool_skill_path_is_compatible_with_source(
            &source,
            &target,
            Some(source_checksum.as_str())
        ));
    }

    #[test]
    fn copy_dir_recursive_creates_destination_root_before_copying_files() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(db);
        let src = temp.path().join("source");
        let dst = temp.path().join("missing-destination");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join(".gitignore"), "target\n").unwrap();

        let mut copied = 0;
        manager.copy_dir_recursive(&src, &dst, &mut copied).unwrap();

        assert_eq!(copied, 1);
        assert_eq!(
            std::fs::read_to_string(dst.join(".gitignore")).unwrap(),
            "target\n"
        );
    }

    #[test]
    fn confirm_skill_update_uses_staging_path_without_polluting_local_path() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let install_dir = temp.path().join("installed").join("example");
        let staging_dir = temp.path().join("staging").join("example");
        std::fs::create_dir_all(install_dir.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(staging_dir.join("SKILL.md"), "---\nname: example\n---\nnew").unwrap();

        let install_path = install_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-update".to_string();
        skill.installed = true;
        skill.local_path = Some(install_path.clone());
        skill.local_paths = Some(vec![install_path.clone()]);
        skill.staging_path = Some(staging_dir.to_string_lossy().to_string());
        db.save_skill(&skill).unwrap();

        manager
            .confirm_skill_update(&skill.id, true, false)
            .unwrap();

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path.as_deref(), Some(install_path.as_str()));
        assert_eq!(reloaded.local_paths.as_deref(), Some(&[install_path][..]));
        assert_eq!(reloaded.staging_path, None);
        assert_eq!(
            std::fs::read_to_string(install_dir.join("SKILL.md")).unwrap(),
            "---\nname: example\n---\nnew"
        );
    }

    #[test]
    fn cancel_skill_update_uses_staging_path_and_preserves_local_path() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let install_dir = temp.path().join("installed").join("example");
        let staging_dir = temp.path().join("staging").join("example");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(install_dir.join("SKILL.md"), "---\nname: example\n---\nold").unwrap();
        std::fs::write(staging_dir.join("SKILL.md"), "---\nname: example\n---\nnew").unwrap();

        let install_path = install_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-cancel".to_string();
        skill.installed = true;
        skill.local_path = Some(install_path.clone());
        skill.local_paths = Some(vec![install_path.clone()]);
        skill.staging_path = Some(staging_dir.to_string_lossy().to_string());
        db.save_skill(&skill).unwrap();

        manager.cancel_skill_update(&skill.id).unwrap();

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path.as_deref(), Some(install_path.as_str()));
        assert_eq!(reloaded.local_paths.as_deref(), Some(&[install_path][..]));
        assert_eq!(reloaded.staging_path, None);
        assert!(!staging_dir.exists());
        assert_eq!(
            std::fs::read_to_string(install_dir.join("SKILL.md")).unwrap(),
            "---\nname: example\n---\nold"
        );
    }

    #[test]
    fn cancel_skill_update_does_not_rewrite_current_local_path_from_local_paths() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let active_dir = temp.path().join("active").join("example");
        let older_dir = temp.path().join("older").join("example");
        let staging_dir = temp.path().join("staging").join("example");
        std::fs::create_dir_all(&active_dir).unwrap();
        std::fs::create_dir_all(&older_dir).unwrap();
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(
            active_dir.join("SKILL.md"),
            "---\nname: example\n---\nactive",
        )
        .unwrap();
        std::fs::write(older_dir.join("SKILL.md"), "---\nname: example\n---\nolder").unwrap();
        std::fs::write(staging_dir.join("SKILL.md"), "---\nname: example\n---\nnew").unwrap();

        let active_path = active_dir.to_string_lossy().to_string();
        let older_path = older_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-cancel-active".to_string();
        skill.installed = true;
        skill.local_path = Some(active_path.clone());
        skill.local_paths = Some(vec![older_path]);
        skill.staging_path = Some(staging_dir.to_string_lossy().to_string());
        db.save_skill(&skill).unwrap();

        manager.cancel_skill_update(&skill.id).unwrap();

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path.as_deref(), Some(active_path.as_str()));
        assert_eq!(reloaded.staging_path, None);
        assert!(!staging_dir.exists());
    }

    #[test]
    fn cancel_skill_update_clears_state_when_staging_directory_is_already_missing() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let install_dir = temp.path().join("installed").join("example");
        let missing_staging_dir = temp.path().join("staging").join("already-removed");
        std::fs::create_dir_all(&install_dir).unwrap();

        let install_path = install_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-cancel-missing-staging".to_string();
        skill.installed = true;
        skill.local_path = Some(install_path.clone());
        skill.local_paths = Some(vec![install_path.clone()]);
        skill.staging_path = Some(missing_staging_dir.to_string_lossy().to_string());
        db.save_skill(&skill).unwrap();

        manager.cancel_skill_update(&skill.id).unwrap();

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path.as_deref(), Some(install_path.as_str()));
        assert_eq!(reloaded.staging_path, None);
    }

    #[test]
    fn cancel_skill_update_clears_legacy_staging_local_path_without_local_paths() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let staging_dir = temp.path().join("staging").join("example");
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(staging_dir.join("SKILL.md"), "---\nname: example\n---\nnew").unwrap();

        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-cancel-legacy".to_string();
        skill.installed = true;
        skill.local_path = Some(format!(
            "{}{}",
            STAGING_LOCAL_PATH_PREFIX,
            staging_dir.to_string_lossy()
        ));
        skill.local_paths = None;
        skill.staging_path = None;
        db.save_skill(&skill).unwrap();

        manager.cancel_skill_update(&skill.id).unwrap();

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path, None);
        assert_eq!(reloaded.staging_path, None);
        assert!(!staging_dir.exists());
    }

    #[test]
    fn confirm_skill_update_supports_legacy_staging_local_path() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let install_dir = temp.path().join("installed").join("example");
        let staging_dir = temp.path().join("staging").join("example");
        std::fs::create_dir_all(install_dir.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&staging_dir).unwrap();
        std::fs::write(staging_dir.join("SKILL.md"), "---\nname: example\n---\nnew").unwrap();

        let install_path = install_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-confirm-legacy".to_string();
        skill.installed = true;
        skill.local_path = Some(format!(
            "{}{}",
            STAGING_LOCAL_PATH_PREFIX,
            staging_dir.to_string_lossy()
        ));
        skill.local_paths = Some(vec![install_path.clone()]);
        skill.staging_path = None;
        db.save_skill(&skill).unwrap();

        manager
            .confirm_skill_update(&skill.id, true, false)
            .unwrap();

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path.as_deref(), Some(install_path.as_str()));
        assert_eq!(reloaded.staging_path, None);
        assert_eq!(
            std::fs::read_to_string(install_dir.join("SKILL.md")).unwrap(),
            "---\nname: example\n---\nnew"
        );
    }

    #[test]
    fn clear_update_staging_state_best_effort_clears_persisted_staging_path() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let install_dir = temp.path().join("installed").join("example");
        let staging_dir = temp.path().join("staging").join("example");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(&staging_dir).unwrap();

        let install_path = install_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            ".".to_string(),
        );
        skill.id = "skill-clear-staging".to_string();
        skill.installed = true;
        skill.local_path = Some(install_path.clone());
        skill.local_paths = Some(vec![install_path.clone()]);
        skill.staging_path = Some(staging_dir.to_string_lossy().to_string());
        db.save_skill(&skill).unwrap();

        manager.clear_update_staging_state_best_effort(&mut skill);

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.local_path.as_deref(), Some(install_path.as_str()));
        assert_eq!(reloaded.staging_path, None);
    }

    #[test]
    fn migrate_installed_commit_sha_writes_back_and_reports_change() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));

        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            "skills/example".to_string(),
        );
        skill.id = "skill-migrate".to_string();
        skill.installed = true;
        skill.installed_commit_sha = Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string());
        db.save_skill(&skill).unwrap();

        let canonical = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let changed = manager
            .migrate_installed_commit_sha(&skill.id, canonical)
            .unwrap();
        assert!(changed, "首次迁移应返回 true");

        let reloaded = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|s| s.id == skill.id)
            .unwrap();
        assert_eq!(reloaded.installed_commit_sha.as_deref(), Some(canonical));
    }

    #[test]
    fn migrate_installed_commit_sha_is_noop_when_already_canonical() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));

        let canonical = "cccccccccccccccccccccccccccccccccccccccc";
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            "https://github.com/owner/repo".to_string(),
            "skills/example".to_string(),
        );
        skill.id = "skill-noop".to_string();
        skill.installed = true;
        skill.installed_commit_sha = Some(canonical.to_string());
        db.save_skill(&skill).unwrap();

        let changed = manager
            .migrate_installed_commit_sha(&skill.id, canonical)
            .unwrap();
        assert!(!changed, "已是规范 SHA 时不应写库");
    }

    #[test]
    fn migrate_installed_commit_sha_rejects_empty_new_sha() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));

        // 没有 skill 也应直接返回 false，不报错
        let changed = manager
            .migrate_installed_commit_sha("nonexistent", "")
            .unwrap();
        assert!(!changed);
    }

    #[test]
    fn migrate_installed_commit_sha_errors_when_skill_missing() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(db);

        let err = manager
            .migrate_installed_commit_sha("nonexistent", "abcdef1234")
            .unwrap_err();
        assert!(err.to_string().contains("SKILL_NOT_FOUND"));
    }

    #[test]
    fn restore_installation_backup_replaces_partial_target_directory() {
        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join(".example.backup-test");
        let final_dir = temp.path().join("example");

        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("SKILL.md"), "original").unwrap();
        std::fs::create_dir_all(&final_dir).unwrap();
        std::fs::write(final_dir.join("partial.txt"), "partial").unwrap();

        restore_installation_backup(&backup, &final_dir).unwrap();

        assert!(!backup.exists());
        assert_eq!(
            std::fs::read_to_string(final_dir.join("SKILL.md")).unwrap(),
            "original"
        );
        assert!(!final_dir.join("partial.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_failure_preserves_installed_skill_metadata() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let manager = super::SkillManager::new(Arc::clone(&db));
        let protected_parent = temp.path().join("protected");
        let skill_dir = protected_parent.join("example");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "test").unwrap();

        let path = skill_dir.to_string_lossy().to_string();
        let mut skill = crate::models::Skill::new(
            "example".to_string(),
            crate::models::LOCAL_REPOSITORY_URL.to_string(),
            path.clone(),
        );
        skill.id = "uninstall-failure".to_string();
        skill.installed = true;
        skill.local_path = Some(path.clone());
        skill.local_paths = Some(vec![path.clone()]);
        skill.source_path = Some(path.clone());
        db.save_skill(&skill).unwrap();

        std::fs::set_permissions(&protected_parent, std::fs::Permissions::from_mode(0o500))
            .unwrap();
        let result = manager.uninstall_skill(&skill.id);
        std::fs::set_permissions(&protected_parent, std::fs::Permissions::from_mode(0o700))
            .unwrap();

        assert!(result
            .unwrap_err()
            .to_string()
            .contains("UNINSTALL_PARTIAL_FAILURE"));
        let persisted = db
            .get_skills()
            .unwrap()
            .into_iter()
            .find(|item| item.id == skill.id)
            .unwrap();
        assert!(persisted.installed);
        assert_eq!(persisted.local_path.as_deref(), Some(path.as_str()));
        assert_eq!(persisted.local_paths.as_deref(), Some(&[path.clone()][..]));
        assert_eq!(persisted.source_path.as_deref(), Some(path.as_str()));
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;
    fn clean(path: &Path, text: &str) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("SKILL.md"), format!("---\nname: formatter\ndescription: Format local documents when requested.\nlicense: MIT\n---\n{text}\n")).unwrap();
    }
    fn fixture() -> (tempfile::TempDir, Arc<Database>, SkillManager, Skill) {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("db.sqlite")).unwrap());
        let mut manager = SkillManager::new(db.clone());
        manager.skills_dir = temp.path().join("common");
        let mut skill = Skill::new(
            "formatter".into(),
            "https://github.com/example/repo".into(),
            "formatter".into(),
        );
        skill.id = uuid::Uuid::new_v4().to_string();
        db.save_skill(&skill).unwrap();
        (temp, db, manager, skill)
    }
    fn update_session(id: &str) -> (tempfile::TempDir, PathBuf) {
        let root = dirs::cache_dir()
            .unwrap()
            .join("agent-skills-guard/staging");
        std::fs::create_dir_all(&root).unwrap();
        let session = tempfile::Builder::new()
            .prefix("update-")
            .tempdir_in(root)
            .unwrap();
        std::fs::write(session.path().join(UPDATE_SESSION_OWNER), id).unwrap();
        let skill = session.path().join("repo/skills/formatter");
        clean(&skill, "Format text.");
        clean(
            &session.path().join("repo/other-skill"),
            "Other repository content.",
        );
        (session, skill)
    }

    #[test]
    fn update_sessions_clean_entire_repository_on_retry_cancel_and_success() {
        let (temp, db, manager, mut skill) = fixture();
        let installed = temp.path().join("installed");
        clean(&installed, "Original.");
        skill.installed = true;
        skill.local_path = Some(installed.to_string_lossy().into());
        db.save_skill(&skill).unwrap();
        let (first, first_skill) = update_session(&skill.id);
        let first_root = first.path().to_path_buf();
        let mut candidate = skill.clone();
        candidate.staging_path = Some(first_skill.to_string_lossy().into());
        manager
            .persist_update_preparation(&skill, &candidate, first)
            .unwrap();
        let active = db.get_skills().unwrap().remove(0);
        let (second, second_skill) = update_session(&skill.id);
        let second_root = second.path().to_path_buf();
        candidate.staging_path = Some(second_skill.to_string_lossy().into());
        manager
            .persist_update_preparation(&active, &candidate, second)
            .unwrap();
        assert!(!first_root.exists());
        assert!(second_root.join("repo/other-skill").exists());
        manager.cancel_skill_update(&skill.id).unwrap();
        assert!(!second_root.exists());
        let active = db.get_skills().unwrap().remove(0);
        let (third, third_skill) = update_session(&skill.id);
        let third_root = third.path().to_path_buf();
        candidate.staging_path = Some(third_skill.to_string_lossy().into());
        manager
            .persist_update_preparation(&active, &candidate, third)
            .unwrap();
        manager.confirm_skill_update(&skill.id, true, true).unwrap();
        assert!(!third_root.exists());
        assert!(installed.join("SKILL.md").exists());
        let backup_root = dirs::cache_dir()
            .unwrap()
            .join("agent-skills-guard/skill-backups")
            .join(hash_bytes(skill.id.as_bytes()));
        std::fs::remove_dir_all(backup_root).unwrap();
    }

    #[test]
    fn update_session_cleanup_checks_owner_and_preserves_shared_siblings() {
        let (_temp, _db, _manager, skill) = fixture();
        let (session, path) = update_session(&skill.id);
        assert!(cleanup_update_staging(&path, "different-skill").is_err());
        assert!(path.exists());
        std::fs::remove_file(session.path().join(UPDATE_SESSION_OWNER)).unwrap();
        cleanup_update_staging(&path, &skill.id).unwrap();
        assert!(!path.exists());
        assert!(session.path().join("repo/other-skill").exists());
    }

    #[test]
    fn default_install_and_sync_refuse_unrelated_same_name_directory() {
        let (temp, db, manager, mut skill) = fixture();
        let target = manager.skills_dir.join("formatter");
        clean(&target, "Keep existing content.");
        let stage = temp.path().join("stage/formatter");
        clean(&stage, "New content.");
        let before = super::super::content_manifest::manifest(&target).unwrap();
        skill.staging_path = Some(stage.to_string_lossy().into_owned());
        db.save_skill(&skill).unwrap();
        assert!(manager
            .confirm_skill_installation(&skill.id, None, false, vec![])
            .unwrap_err()
            .to_string()
            .contains("INSTALL_TARGET_EXISTS"));
        skill.installed = true;
        skill.is_local_only = true;
        skill.local_path = Some(stage.to_string_lossy().into_owned());
        db.save_skill(&skill).unwrap();
        assert!(manager.sync_skill_to_tools(&skill.id, vec![]).is_err());
        assert_eq!(
            super::super::content_manifest::manifest(&target).unwrap(),
            before
        );
        assert!(stage.is_dir() && !stage.symlink_metadata().unwrap().file_type().is_symlink());
    }
    #[test]
    fn rejected_update_keeps_live_files_and_failed_install_cleans_temp() {
        let (temp, db, manager, mut skill) = fixture();
        let live = temp.path().join("live/formatter");
        let stage = temp.path().join("stage/formatter");
        clean(&live, "Keep existing content.");
        clean(&stage, "New content.");
        std::fs::write(stage.join("payload.sh"), "rm -rf /\n").unwrap();
        skill.installed = true;
        skill.local_path = Some(live.to_string_lossy().into_owned());
        skill.local_paths = Some(vec![live.to_string_lossy().into_owned()]);
        skill.staging_path = Some(stage.to_string_lossy().into_owned());
        db.save_skill(&skill).unwrap();
        let before = super::super::content_manifest::manifest(&live).unwrap();
        assert!(manager
            .confirm_skill_update(&skill.id, true, false)
            .unwrap_err()
            .to_string()
            .contains("SECURITY_CHECK_BLOCKED"));
        assert_eq!(
            before,
            super::super::content_manifest::manifest(&live).unwrap()
        );
        assert!(manager
            .confirm_skill_installation(&skill.id, None, false, vec![])
            .is_err());
        assert_eq!(std::fs::read_dir(&manager.skills_dir).unwrap().count(), 0);
    }
    #[test]
    fn cancel_installation_preserves_active_version_and_candidate_is_separate() {
        let (temp, db, manager, mut skill) = fixture();
        let live = temp.path().join("live");
        clean(&live, "Old content.");
        skill.installed = true;
        skill.local_path = Some(live.to_string_lossy().into_owned());
        skill.installed_commit_sha = Some("old-sha".into());
        skill.security_score = Some(73);
        skill.staging_path = Some(temp.path().join("stage").to_string_lossy().into_owned());
        db.save_skill(&skill).unwrap();
        let mut candidate = skill.clone();
        candidate.installed_commit_sha = Some("new-sha".into());
        candidate.security_score = Some(100);
        db.save_skill_preparation(&candidate).unwrap();
        manager.cancel_skill_installation(&skill.id).unwrap();
        let saved = db.get_skills().unwrap().remove(0);
        assert!(saved.installed);
        assert_eq!(saved.local_path, skill.local_path);
        assert_eq!(saved.installed_commit_sha, skill.installed_commit_sha);
        assert_eq!(saved.security_score, Some(73));
        assert!(db.get_skill_preparation(&skill.id).unwrap().is_none());
    }
    #[test]
    fn baseline_detects_deletions_and_survives_unrelated_cache_refresh() {
        let (temp, db, manager, mut skill) = fixture();
        let live = temp.path().join("live");
        clean(&live, "Installed content.");
        std::fs::write(live.join("optional.py"), "print('local')").unwrap();
        skill.local_path = Some(live.to_string_lossy().into_owned());
        let baseline = super::super::content_manifest::manifest(&live).unwrap();
        db.save_skill_baseline(&skill.id, &baseline).unwrap();
        let cache = temp.path().join("cache");
        clean(&cache, "Changed upstream content.");
        assert!(manager.installed_modifications(&skill).unwrap().is_empty());
        std::fs::remove_file(live.join("optional.py")).unwrap();
        assert_eq!(
            manager.installed_modifications(&skill).unwrap(),
            vec!["Deleted: optional.py"]
        );
        assert!(manager
            .detect_local_modifications(&live, &cache)
            .unwrap()
            .iter()
            .any(|p| p.contains("SKILL.md")));
        skill.id = "legacy-with-no-baseline".into();
        assert!(!manager.installed_modifications(&skill).unwrap().is_empty());
    }
    #[test]
    fn database_commit_failure_rolls_back_updated_files() {
        let (temp, db, manager, mut skill) = fixture();
        let live = temp.path().join("live/formatter");
        let stage = temp.path().join("stage/formatter");
        clean(&live, "Old content.");
        clean(&stage, "New content.");
        skill.installed = true;
        skill.local_path = Some(live.to_string_lossy().into_owned());
        skill.local_paths = Some(vec![live.to_string_lossy().into_owned()]);
        skill.staging_path = Some(stage.to_string_lossy().into_owned());
        db.save_skill(&skill).unwrap();
        let before = super::super::content_manifest::manifest(&live).unwrap();
        let conn = rusqlite::Connection::open(temp.path().join("db.sqlite")).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_update BEFORE INSERT ON skills BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
        assert!(manager
            .confirm_skill_update(&skill.id, true, false)
            .is_err());
        assert_eq!(
            super::super::content_manifest::manifest(&live).unwrap(),
            before
        );
        assert_eq!(db.get_skills().unwrap()[0].staging_path, skill.staging_path);
    }
    #[test]
    fn same_named_skills_keep_distinct_recovery_backups() {
        let (temp, db, manager, mut first) = fixture();
        let mut second = first.clone();
        second.id = uuid::Uuid::new_v4().to_string();
        for (index, skill) in [&mut first, &mut second].into_iter().enumerate() {
            let live = temp.path().join(format!("live-{index}/formatter"));
            let stage = temp.path().join(format!("stage-{index}/formatter"));
            clean(&live, &format!("Old content {index}."));
            clean(&stage, "New content.");
            skill.installed = true;
            skill.local_path = Some(live.to_string_lossy().into_owned());
            skill.local_paths = Some(vec![live.to_string_lossy().into_owned()]);
            skill.staging_path = Some(stage.to_string_lossy().into_owned());
            db.save_skill(skill).unwrap();
            manager
                .confirm_skill_update(&skill.id, true, false)
                .unwrap();
            let backup_root = dirs::cache_dir()
                .unwrap()
                .join("agent-skills-guard/skill-backups")
                .join(hash_bytes(skill.id.as_bytes()));
            let backup = std::fs::read_dir(&backup_root)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            assert!(std::fs::read_to_string(backup.join("SKILL.md"))
                .unwrap()
                .contains(&format!("Old content {index}")));
            std::fs::remove_dir_all(backup_root).unwrap();
            assert!(db.get_skill_baseline(&skill.id).unwrap().is_some());
        }
    }
}
