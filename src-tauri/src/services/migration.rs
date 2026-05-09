use crate::models::{Skill, LOCAL_REPOSITORY_URL};
use crate::services::{AgentTool, Database};
use anyhow::{Context, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const ADOPT_EXISTING_SKILLS_MIGRATION: &str = "adopt-existing-skills-v1";

static STARTUP_MIGRATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct ToolSkillDir {
    pub tool_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SkillAdoptionSummary {
    pub discovered: usize,
    pub created: usize,
    pub updated: usize,
}

pub struct MigrationManager {
    db: Arc<Database>,
}

impl MigrationManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn run_startup_migrations(&self) -> Result<SkillAdoptionSummary> {
        let _guard = STARTUP_MIGRATION_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let tool_dirs = AgentTool::all()
            .into_iter()
            .filter_map(|tool| {
                tool.default_skills_dir().map(|path| ToolSkillDir {
                    tool_id: tool.id().to_string(),
                    path,
                })
            })
            .collect::<Vec<_>>();
        self.run_startup_migrations_from_dirs(&tool_dirs)
    }

    pub fn run_startup_migrations_from_dirs(
        &self,
        tool_dirs: &[ToolSkillDir],
    ) -> Result<SkillAdoptionSummary> {
        let summary = self.adopt_existing_skills_from_dirs(tool_dirs)?;
        if !self
            .db
            .is_app_migration_completed(ADOPT_EXISTING_SKILLS_MIGRATION)?
        {
            self.db
                .mark_app_migration_completed(ADOPT_EXISTING_SKILLS_MIGRATION)?;
        }
        Ok(summary)
    }

    pub fn adopt_existing_skills_from_dirs(
        &self,
        tool_dirs: &[ToolSkillDir],
    ) -> Result<SkillAdoptionSummary> {
        let inventory = discover_skill_inventory(tool_dirs)?;
        let existing_skills = self.db.get_skills()?;
        let mut summary = SkillAdoptionSummary {
            discovered: inventory.len(),
            ..SkillAdoptionSummary::default()
        };

        for group in inventory {
            if let Some(mut skill) = find_existing_skill_for_group(&existing_skills, &group) {
                let mut changed = false;
                if !skill.installed {
                    skill.installed = true;
                    skill.installed_at = Some(Utc::now());
                    changed = true;
                }

                let merged_paths = merge_paths(skill.local_paths.clone(), &group.display_paths);
                if skill.local_paths.as_ref() != Some(&merged_paths) {
                    skill.local_paths = Some(merged_paths);
                    changed = true;
                }

                let source_path = choose_source_path(&group);
                if skill.source_path.as_deref() != Some(source_path.as_str()) {
                    skill.source_path = Some(source_path.clone());
                    changed = true;
                }
                if skill.local_path.as_deref() != Some(source_path.as_str()) {
                    skill.local_path = Some(source_path);
                    changed = true;
                }

                let merged_tools = merge_tools(&skill.linked_tools, &group.tool_ids);
                if skill.linked_tools != merged_tools {
                    skill.linked_tools = merged_tools;
                    changed = true;
                }

                if skill.repository_url != LOCAL_REPOSITORY_URL && skill.is_local_only {
                    skill.is_local_only = false;
                    changed = true;
                }

                if changed {
                    self.db.save_skill(&skill)?;
                    summary.updated += 1;
                }
                continue;
            }

            let source_path = choose_source_path(&group);
            let mut skill =
                Skill::new(group.name.clone(), LOCAL_REPOSITORY_URL.to_string(), source_path.clone());
            skill.id = build_local_skill_id(&group.checksum, &group.canonical_key);
            skill.description = group.description.clone();
            skill.installed = true;
            skill.installed_at = Some(Utc::now());
            skill.local_path = Some(source_path.clone());
            skill.local_paths = Some(group.display_paths.clone());
            skill.checksum = Some(group.checksum.clone());
            skill.source_path = Some(source_path);
            skill.linked_tools = merge_tools(&[], &group.tool_ids);
            skill.is_local_only = true;

            self.db.save_skill(&skill)?;
            summary.created += 1;
        }

        Ok(summary)
    }
}

#[derive(Debug, Clone)]
struct InventoryGroup {
    canonical_key: String,
    display_paths: Vec<String>,
    tool_ids: Vec<String>,
    checksum: String,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Clone)]
struct InventoryEntry {
    canonical_key: String,
    display_path: String,
    tool_id: String,
    checksum: String,
    name: String,
    description: Option<String>,
}

fn discover_skill_inventory(tool_dirs: &[ToolSkillDir]) -> Result<Vec<InventoryGroup>> {
    let mut by_canonical: HashMap<String, InventoryGroup> = HashMap::new();

    for tool_dir in tool_dirs {
        if !tool_dir.path.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&tool_dir.path)
            .with_context(|| format!("无法读取工具技能目录: {:?}", tool_dir.path))?
        {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            let content = std::fs::read_to_string(&skill_md)
                .with_context(|| format!("无法读取技能文件: {:?}", skill_md))?;
            let checksum = calculate_checksum(content.as_bytes());
            let (name, description) = parse_skill_frontmatter(&content).unwrap_or_else(|| {
                (
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    None,
                )
            });
            let canonical_key = canonical_key(&path);
            let display_path = path.to_string_lossy().to_string();
            let entry = InventoryEntry {
                canonical_key: canonical_key.clone(),
                display_path,
                tool_id: tool_dir.tool_id.clone(),
                checksum,
                name,
                description,
            };

            by_canonical
                .entry(canonical_key)
                .and_modify(|group| merge_entry_into_group(group, &entry))
                .or_insert_with(|| InventoryGroup {
                    canonical_key: entry.canonical_key,
                    display_paths: vec![entry.display_path],
                    tool_ids: vec![entry.tool_id],
                    checksum: entry.checksum,
                    name: entry.name,
                    description: entry.description,
                });
        }
    }

    Ok(by_canonical.into_values().collect())
}

fn merge_entry_into_group(group: &mut InventoryGroup, entry: &InventoryEntry) {
    if !group.display_paths.contains(&entry.display_path) {
        group.display_paths.push(entry.display_path.clone());
    }
    if !group.tool_ids.contains(&entry.tool_id) {
        group.tool_ids.push(entry.tool_id.clone());
    }
    if group.description.is_none() {
        group.description = entry.description.clone();
    }
}

fn find_existing_skill_for_group(
    existing_skills: &[Skill],
    group: &InventoryGroup,
) -> Option<Skill> {
    existing_skills
        .iter()
        .find(|skill| skill_matches_group(skill, group))
        .cloned()
}

fn skill_matches_group(skill: &Skill, group: &InventoryGroup) -> bool {
    skill_paths(skill).iter().any(|path| {
        group.display_paths.contains(path) || canonical_key(Path::new(path)) == group.canonical_key
    })
}

fn skill_paths(skill: &Skill) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(path) = &skill.local_path {
        paths.push(path.clone());
    }
    if let Some(local_paths) = &skill.local_paths {
        paths.extend(local_paths.clone());
    }
    if let Some(path) = &skill.source_path {
        paths.push(path.clone());
    }
    paths.sort();
    paths.dedup();
    paths
}

fn merge_paths(existing: Option<Vec<String>>, discovered: &[String]) -> Vec<String> {
    let mut paths = existing.unwrap_or_default();
    for path in discovered {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths
}

fn merge_tools(existing: &[String], discovered: &[String]) -> Vec<String> {
    let mut tools = BTreeSet::new();
    for tool in existing.iter().chain(discovered.iter()) {
        if tool != "agents" {
            tools.insert(tool.clone());
        }
    }
    tools.into_iter().collect()
}

fn choose_source_path(group: &InventoryGroup) -> String {
    group
        .display_paths
        .iter()
        .find(|path| path.replace('\\', "/").contains("/.agents/skills/"))
        .cloned()
        .unwrap_or_else(|| group.display_paths[0].clone())
}

fn canonical_key(path: &Path) -> String {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut key = path.to_string_lossy().replace('\\', "/");
    while key.ends_with('/') && key.len() > 1 {
        key.pop();
    }
    if cfg!(windows) {
        key = key.to_ascii_lowercase();
    }
    key
}

fn calculate_checksum(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn build_local_skill_id(checksum: &str, canonical_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(checksum.as_bytes());
    hasher.update(b":");
    hasher.update(canonical_path.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("local::{}", &digest[..16])
}

fn parse_skill_frontmatter(content: &str) -> Option<(String, Option<String>)> {
    if !content.starts_with("---") {
        return None;
    }

    // This migration only needs best-effort name/description extraction. It intentionally
    // uses a small parser and may stop at an embedded YAML document separator.
    let end = content[3..].find("---")? + 3;
    let frontmatter = &content[3..end];
    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        } else if let Some(value) = line.strip_prefix("description:") {
            description = Some(
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            );
        }
    }

    name.map(|name| (name, description))
}

#[cfg(test)]
mod tests {
    use super::{MigrationManager, ToolSkillDir};
    use crate::models::Skill;
    use crate::services::{link_fs, Database};
    use std::sync::Arc;

    #[test]
    fn adoption_marks_linked_claude_directory_skill_as_claude_code_without_moving_files() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let real_skills = temp.path().join("real-claude-skills");
        let claude_skills = temp.path().join("home").join(".claude").join("skills");
        let skill_dir = real_skills.join("example");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: example\ndescription: Existing Claude skill\n---\n",
        )
        .unwrap();
        link_fs::create_dir_link(&real_skills, &claude_skills).unwrap();

        let manager = MigrationManager::new(Arc::clone(&db));
        let summary = manager
            .adopt_existing_skills_from_dirs(&[ToolSkillDir {
                tool_id: "claude-code".to_string(),
                path: claude_skills.clone(),
            }])
            .unwrap();

        let skills = db.get_skills().unwrap();
        assert_eq!(summary.discovered, 1);
        assert_eq!(summary.created, 1);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "example");
        assert_eq!(skills[0].linked_tools, vec!["claude-code".to_string()]);
        assert!(skills[0].is_local_only);
        assert_eq!(
            skills[0].local_path.as_deref(),
            Some(claude_skills.join("example").to_string_lossy().as_ref())
        );
        assert!(
            skill_dir.exists(),
            "adoption must not move the real skill directory"
        );
    }

    #[test]
    fn adoption_marks_child_junction_skill_as_linked_tool_for_every_tool() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let agents_skills = temp.path().join("home").join(".agents").join("skills");
        let agents_skill_dir = agents_skills.join("example");
        std::fs::create_dir_all(&agents_skill_dir).unwrap();
        std::fs::write(
            agents_skill_dir.join("SKILL.md"),
            "---\nname: example\ndescription: Shared skill\n---\n",
        )
        .unwrap();

        let tool_dirs = [
            (
                "claude-code",
                temp.path().join("home").join(".claude").join("skills"),
            ),
            (
                "codex",
                temp.path().join("home").join(".codex").join("skills"),
            ),
            (
                "antigravity",
                temp.path()
                    .join("home")
                    .join(".gemini")
                    .join("antigravity")
                    .join("skills"),
            ),
            (
                "opencode",
                temp.path()
                    .join("home")
                    .join(".config")
                    .join("opencode")
                    .join("skills"),
            ),
        ];
        for (_, tool_dir) in &tool_dirs {
            std::fs::create_dir_all(tool_dir).unwrap();
            link_fs::create_dir_link(&agents_skill_dir, &tool_dir.join("example")).unwrap();
        }

        let mut adoption_dirs = vec![ToolSkillDir {
            tool_id: "agents".to_string(),
            path: agents_skills.clone(),
        }];
        adoption_dirs.extend(tool_dirs.iter().map(|(tool_id, path)| ToolSkillDir {
            tool_id: (*tool_id).to_string(),
            path: path.clone(),
        }));

        let manager = MigrationManager::new(Arc::clone(&db));
        let summary = manager
            .adopt_existing_skills_from_dirs(&adoption_dirs)
            .unwrap();

        let skills = db.get_skills().unwrap();
        assert_eq!(summary.discovered, 1);
        assert_eq!(summary.created, 1);
        assert_eq!(skills.len(), 1);
        assert_eq!(
            skills[0].linked_tools,
            vec![
                "antigravity".to_string(),
                "claude-code".to_string(),
                "codex".to_string(),
                "opencode".to_string()
            ]
        );
        assert_eq!(
            skills[0].source_path.as_deref(),
            Some(agents_skill_dir.to_string_lossy().as_ref())
        );
        let local_paths = skills[0].local_paths.as_ref().unwrap();
        assert!(local_paths.contains(&agents_skill_dir.to_string_lossy().to_string()));
        for (_, tool_dir) in &tool_dirs {
            assert!(local_paths.contains(&tool_dir.join("example").to_string_lossy().to_string()));
        }
    }

    #[test]
    fn completed_startup_migration_still_adopts_new_local_skills() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let agents_skills = temp.path().join("home").join(".agents").join("skills");
        let skill_dir = agents_skills.join("example");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: example\n---\n").unwrap();
        db.mark_app_migration_completed(super::ADOPT_EXISTING_SKILLS_MIGRATION)
            .unwrap();

        let manager = MigrationManager::new(Arc::clone(&db));
        let summary = manager
            .run_startup_migrations_from_dirs(&[ToolSkillDir {
                tool_id: "agents".to_string(),
                path: agents_skills,
            }])
            .unwrap();

        assert_eq!(summary.discovered, 1);
        assert_eq!(summary.created, 1);
        assert_eq!(db.get_skills().unwrap().len(), 1);
    }

    #[test]
    fn adoption_backfills_old_managed_skill_record_without_marking_it_local_only() {
        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(temp.path().join("test.db")).unwrap());
        let claude_skills = temp.path().join("home").join(".claude").join("skills");
        let skill_dir = claude_skills.join("repo-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: repo-skill\n---\n").unwrap();

        let mut old_skill = Skill::new(
            "repo-skill".to_string(),
            "https://github.com/example/repo".to_string(),
            "skills/repo-skill".to_string(),
        );
        old_skill.installed = true;
        old_skill.local_path = Some(skill_dir.to_string_lossy().to_string());
        old_skill.is_local_only = false;
        db.save_skill(&old_skill).unwrap();

        let manager = MigrationManager::new(Arc::clone(&db));
        let summary = manager
            .adopt_existing_skills_from_dirs(&[ToolSkillDir {
                tool_id: "claude-code".to_string(),
                path: claude_skills,
            }])
            .unwrap();

        let skills = db.get_skills().unwrap();
        assert_eq!(summary.discovered, 1);
        assert_eq!(summary.updated, 1);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, old_skill.id);
        assert_eq!(skills[0].repository_url, "https://github.com/example/repo");
        assert_eq!(skills[0].linked_tools, vec!["claude-code".to_string()]);
        assert!(!skills[0].is_local_only);
        assert_eq!(
            skills[0].source_path.as_deref(),
            Some(skill_dir.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn database_records_completed_migrations_idempotently() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::new(temp.path().join("test.db")).unwrap();

        assert!(!db
            .is_app_migration_completed("adopt-existing-skills-v1")
            .unwrap());

        db.mark_app_migration_completed("adopt-existing-skills-v1")
            .unwrap();
        db.mark_app_migration_completed("adopt-existing-skills-v1")
            .unwrap();

        assert!(db
            .is_app_migration_completed("adopt-existing-skills-v1")
            .unwrap());
    }
}
