use crate::models::Skill;
use crate::security::{ScanOptions, SecurityScanner};
use crate::services::{Database, GitHubService};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;

pub struct SkillManager {
    db: Arc<Database>,
    github: GitHubService,
    scanner: SecurityScanner,
    skills_dir: PathBuf,
}

impl SkillManager {
    pub fn new(db: Arc<Database>) -> Self {
        let skills_dir = Self::get_skills_directory();

        Self {
            db,
            github: GitHubService::new(),
            scanner: SecurityScanner::new(),
            skills_dir,
        }
    }

    /// 获取 skills 安装目录
    fn get_skills_directory() -> PathBuf {
        let home = dirs::home_dir().expect("Failed to get home directory");
        home.join(".claude").join("skills")
    }

    /// 下载并分析 skill，返回文件内容和安全报告
    pub async fn download_and_analyze(
        &self,
        skill: &mut Skill,
    ) -> Result<(Vec<u8>, crate::models::SecurityReport)> {
        // 构建下载 URL
        let (owner, repo) = crate::models::Repository::from_github_url(&skill.repository_url)?;

        // 尝试多个分支下载 SKILL.md 文件
        let branches = ["main", "master"];
        let mut content = None;
        let mut last_error = None;

        for branch in branches.iter() {
            let download_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md",
                owner, repo, branch, skill.file_path
            );

            log::info!("尝试从分支 {} 下载 SKILL.md: {}", branch, download_url);

            match self.github.download_file(&download_url).await {
                Ok(file_content) => {
                    log::info!("成功从分支 {} 下载 SKILL.md", branch);
                    content = Some(file_content);
                    break;
                }
                Err(e) => {
                    log::info!("分支 {} 下载失败: {}", branch, e);
                    last_error = Some(e);
                    continue;
                }
            }
        }

        let content = content.ok_or_else(|| {
            last_error.unwrap_or_else(|| anyhow::anyhow!("所有分支均无法下载 SKILL.md"))
        })?;

        // 解析 frontmatter 更新 skill 元数据
        let (name, description) = self
            .github
            .fetch_skill_metadata(&owner, &repo, &skill.file_path)
            .await?;
        skill.name = name;
        skill.description = description;

        // 安全扫描
        let content_str = String::from_utf8_lossy(&content);
        let report = self.scanner.scan_file(&content_str, "SKILL.md", "zh")?;

        // 更新 skill 信息
        skill.security_score = Some(report.score);
        skill.security_level = Some(report.level.as_str().to_string());
        skill.security_issues = Some(
            report
                .issues
                .iter()
                .map(|i| format!("{:?}: {}", i.severity, i.description))
                .collect(),
        );
        skill.scanned_at = Some(Utc::now());
        skill.checksum = Some(self.scanner.calculate_checksum(&content));

        Ok((content, report))
    }

    /// 安装 skill 到本地
    pub async fn install_skill(&self, skill_id: &str, install_path: Option<String>) -> Result<()> {
        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能，请检查技能是否存在")?;

        // 获取对应的仓库记录以获取缓存路径
        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("未找到对应的仓库记录")?;

        // 确定安装基础目录（使用自定义路径或默认路径）
        let install_base_dir = if let Some(user_path) = install_path {
            PathBuf::from(user_path)
        } else {
            self.skills_dir.clone()
        };

        // 确保目标目录存在
        std::fs::create_dir_all(&install_base_dir).context("无法创建技能目录，请检查磁盘权限")?;

        // 创建 skill 文件夹（使用 skill 的文件夹名）
        // 如果 file_path 是 "."（位于仓库根目录），使用技能名称作为文件夹名
        let skill_folder_name = if skill.file_path == "." {
            log::info!(
                "技能位于仓库根目录，使用技能名称作为文件夹名: {}",
                skill.name
            );
            skill.name.clone()
        } else {
            PathBuf::from(&skill.file_path)
                .file_name()
                .context("技能路径格式无效")?
                .to_str()
                .context("技能文件夹名称包含无效字符")?
                .to_string()
        };

        let skill_dir = install_base_dir.join(&skill_folder_name);

        // 如果目标目录已存在，先清理（避免旧文件冲突）
        if skill_dir.exists() {
            log::info!("目标目录已存在，先清理: {:?}", skill_dir);
            std::fs::remove_dir_all(&skill_dir).context("无法清理现有技能目录")?;
        }

        std::fs::create_dir_all(&skill_dir).context("无法创建技能子目录，请检查磁盘空间和权限")?;

        // 优先从本地缓存复制文件
        if let Some(cache_path) = &repo.cache_path {
            let cache_path_buf = PathBuf::from(cache_path);

            // 在缓存中找到技能目录
            // cache_path 指向 extracted 目录，需要进入仓库的第一层目录（GitHub 压缩包格式）
            let cache_entries = std::fs::read_dir(&cache_path_buf).context("无法读取缓存目录")?;

            let mut repo_root = None;
            for entry in cache_entries {
                if let Ok(entry) = entry {
                    if entry.path().is_dir() {
                        repo_root = Some(entry.path());
                        break;
                    }
                }
            }

            if let Some(repo_root) = repo_root {
                let cached_skill_dir = if skill.file_path == "." {
                    repo_root.clone()
                } else {
                    repo_root.join(&skill.file_path)
                };

                if cached_skill_dir.exists() {
                    log::info!("从本地缓存复制文件: {:?}", cached_skill_dir);

                    // 复制整个目录
                    self.copy_directory(&cached_skill_dir, &skill_dir)
                        .context("从缓存复制文件失败")?;

                    log::info!("成功从本地缓存安装技能");
                } else {
                    log::warn!("缓存中未找到技能目录，降级使用网络下载");
                    self.install_from_network(&skill, &skill_dir).await?;
                }
            } else {
                log::warn!("缓存目录格式异常，降级使用网络下载");
                self.install_from_network(&skill, &skill_dir).await?;
            }
        } else {
            log::info!("仓库未缓存，使用网络下载");
            self.install_from_network(&skill, &skill_dir).await?;
        }

        // 从缓存读取 SKILL.md 进行元数据提取
        let skill_md_path = skill_dir.join("SKILL.md");
        if skill_md_path.exists() {
            let skill_md_content =
                std::fs::read_to_string(&skill_md_path).context("读取 SKILL.md 失败")?;

            // 解析 frontmatter
            if let Ok((name, description)) = self.github.parse_skill_frontmatter(&skill_md_content)
            {
                skill.name = name;
                skill.description = description;
            }
        }

        // 扫描整个技能目录
        let scan_report = self.scanner.scan_directory_with_options(
            skill_dir.to_str().context("技能目录路径无效")?,
            &skill.id,
            "zh",
            ScanOptions { skip_readme: true },
            None,
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        // 检查是否被 hard_trigger 阻止
        if scan_report.blocked {
            // 先删除已下载的文件
            if skill_dir.exists() {
                std::fs::remove_dir_all(&skill_dir)?;
            }

            let mut error_msg =
                format!("⛔ 安全检测发现严重威胁，禁止安装！\n\n检测到以下高危操作：\n");
            for (idx, issue) in scan_report.hard_trigger_issues.iter().enumerate() {
                error_msg.push_str(&format!("{}. {}\n", idx + 1, issue));
            }
            error_msg.push_str("\n这些操作可能对您的系统造成严重危害，强烈建议不要安装此技能。");
            anyhow::bail!(error_msg);
        }

        // 注释掉评分检查，允许用户在前端确认后安装低分技能
        // 只有硬触发的技能会被后端强制阻止
        // if scan_report.score < 50 {
        //     // 先删除已下载的文件
        //     if skill_dir.exists() {
        //         std::fs::remove_dir_all(&skill_dir)?;
        //     }
        //
        //     anyhow::bail!(
        //         "技能安全评分过低 ({}分)，为保护您的安全已阻止安装。建议评分至少为 50 分以上。\n\n扫描了 {} 个文件：{}",
        //         scan_report.score,
        //         scan_report.scanned_files.len(),
        //         scan_report.scanned_files.join(", ")
        //     );
        // }

        // 更新 skill 安全信息
        skill.security_score = Some(scan_report.score);
        skill.security_level = Some(scan_report.level.as_str().to_string());
        skill.security_issues = Some(
            scan_report
                .issues
                .iter()
                .map(|i| {
                    let file_info = i
                        .file_path
                        .as_ref()
                        .map(|f| format!("[{}] ", f))
                        .unwrap_or_default();
                    format!("{}{:?}: {}", file_info, i.severity, i.description)
                })
                .collect(),
        );
        skill.scanned_at = Some(Utc::now());

        // 更新数据库
        let new_path = skill_dir.to_string_lossy().to_string();

        // 将新路径添加到 local_paths 数组中
        let mut paths = skill.local_paths.clone().unwrap_or_default();
        if !paths.contains(&new_path) {
            paths.push(new_path.clone());
        }
        skill.local_paths = Some(paths);

        // 更新 installed 状态和时间
        skill.installed = true;
        skill.installed_at = Some(Utc::now());
        skill.local_path = Some(new_path); // 保持向后兼容,存储最新的路径

        self.db.save_skill(&skill)?;

        log::info!("Skill installed successfully: {}", skill.name);
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
            .context("未找到该技能")?;

        // 下载并分析 SKILL.md
        let (_skill_md_content, _report) = self.download_and_analyze(&mut skill).await?;

        // 获取仓库记录
        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("未找到对应的仓库记录")?
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

        // 直接扫描缓存中的技能目录
        let scan_report = self.scanner.scan_directory_with_options(
            skill_cache_dir.to_str().context("技能目录路径无效")?,
            &skill.id,
            locale,
            ScanOptions { skip_readme: true },
            None,
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        // 更新 skill 安全信息到数据库（但不标记为已安装）
        skill.security_score = Some(scan_report.score);
        skill.security_level = Some(scan_report.level.as_str().to_string());
        skill.security_issues = Some(
            scan_report
                .issues
                .iter()
                .map(|i| {
                    let file_info = i
                        .file_path
                        .as_ref()
                        .map(|f| format!("[{}] ", f))
                        .unwrap_or_default();
                    format!("{}{:?}: {}", file_info, i.severity, i.description)
                })
                .collect(),
        );
        skill.scanned_at = Some(Utc::now());
        // 注意：这里暂时保存缓存路径，确认安装时会更新为实际安装路径
        skill.local_path = Some(skill_cache_dir.to_string_lossy().to_string());

        // 保存安全信息到数据库，但不标记为已安装
        self.db.save_skill(&skill)?;

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
            .context("无法获取系统缓存目录")?
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
            repo_root.join(skill_file_path)
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

        anyhow::bail!("未找到仓库根目录")
    }

    /// 递归复制目录
    fn copy_dir_recursive(
        &self,
        src: &std::path::Path,
        dst: &std::path::Path,
        counter: &mut usize,
    ) -> Result<()> {
        use anyhow::Context;

        for entry in std::fs::read_dir(src).context(format!("无法读取源目录: {:?}", src))? {
            let entry = entry?;
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);

            if src_path.is_dir() {
                std::fs::create_dir_all(&dst_path)
                    .context(format!("无法创建目标目录: {:?}", dst_path))?;
                self.copy_dir_recursive(&src_path, &dst_path, counter)?;
            } else {
                std::fs::copy(&src_path, &dst_path)
                    .context(format!("无法复制文件: {:?} -> {:?}", src_path, dst_path))?;
                *counter += 1;
                log::debug!("Copied file: {:?}", file_name);
            }
        }

        Ok(())
    }

    /// 确认安装技能：从缓存复制到目标路径，标记为已安装
    pub fn confirm_skill_installation(
        &self,
        skill_id: &str,
        install_path: Option<String>,
    ) -> Result<()> {
        use anyhow::Context;
        use std::path::PathBuf;

        log::info!("Confirming installation for skill: {}", skill_id);

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 获取缓存中的技能路径（prepare阶段保存的）
        let cache_path = skill
            .local_path
            .as_ref()
            .context("技能尚未准备，请先调用prepare_skill_installation")?;
        let cache_dir = PathBuf::from(cache_path);

        // 获取仓库的 cached_commit_sha
        let repositories = self.db.get_repositories()?;
        let repo = repositories.iter().find(|r| r.url == skill.repository_url);
        let commit_sha = repo.and_then(|r| r.cached_commit_sha.clone());

        // 确定最终安装路径
        let install_base_dir = if let Some(user_path) = install_path {
            PathBuf::from(user_path)
        } else {
            self.skills_dir.clone()
        };

        // 获取技能目录名
        let skill_dir_name = cache_dir.file_name().context("无效的技能目录名")?;
        let final_install_dir = install_base_dir.join(skill_dir_name);

        // 确保目标基础目录存在
        std::fs::create_dir_all(&install_base_dir).context("无法创建目标目录")?;

        // 如果目标目录已存在，先删除
        if final_install_dir.exists() {
            std::fs::remove_dir_all(&final_install_dir).context("无法删除已存在的目标目录")?;
        }

        // 创建目标目录
        std::fs::create_dir_all(&final_install_dir).context("无法创建最终安装目录")?;

        // 从缓存复制到目标路径
        log::info!(
            "Copying skill from cache {:?} to {:?}",
            cache_dir,
            final_install_dir
        );
        let mut files_copied = 0;
        self.copy_dir_recursive(&cache_dir, &final_install_dir, &mut files_copied)?;

        log::info!(
            "Copied {} files from cache to install directory",
            files_copied
        );

        // 更新安装路径
        let install_path_str = final_install_dir.to_string_lossy().to_string();

        // 更新 local_path（向后兼容）
        skill.local_path = Some(install_path_str.clone());

        // 更新 local_paths 数组（支持多路径安装）
        let mut paths = skill.local_paths.clone().unwrap_or_default();
        if !paths.contains(&install_path_str) {
            paths.push(install_path_str);
        }
        skill.local_paths = Some(paths);

        // 标记为已安装
        skill.installed = true;
        skill.installed_at = Some(Utc::now());
        skill.installed_commit_sha = commit_sha;

        self.db.save_skill(&skill)?;

        log::info!("Skill installation confirmed: {}", skill.name);
        Ok(())
    }

    /// 取消安装技能：清除准备阶段的数据（不删除缓存）
    pub fn cancel_skill_installation(&self, skill_id: &str) -> Result<()> {
        use anyhow::Context;

        log::info!("Canceling installation for skill: {}", skill_id);

        let skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 注意：不删除缓存中的文件，因为缓存是共享的仓库缓存
        // 只清除数据库中的准备阶段信息

        // 清除数据库中的安全信息和本地路径
        let mut skill = skill;
        skill.local_path = None;
        skill.security_score = None;
        skill.security_level = None;
        skill.security_issues = None;
        skill.scanned_at = None;

        self.db.save_skill(&skill)?;

        log::info!("Skill installation canceled: {}", skill.name);
        Ok(())
    }

    /// 卸载 skill
    pub fn uninstall_skill(&self, skill_id: &str) -> Result<()> {
        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 删除所有安装路径的文件
        if let Some(local_paths) = &skill.local_paths {
            for local_path in local_paths {
                let path = PathBuf::from(local_path);
                if path.exists() {
                    // 如果是目录，删除整个目录
                    if path.is_dir() {
                        if let Err(e) = std::fs::remove_dir_all(&path) {
                            log::warn!("删除技能目录失败: {:?}, 错误: {}", path, e);
                        }
                    } else {
                        if let Err(e) = std::fs::remove_file(&path) {
                            log::warn!("删除技能文件失败: {:?}, 错误: {}", path, e);
                        }
                    }
                }
            }
        }

        // 向后兼容:如果 local_paths 为空,尝试删除 local_path
        if skill.local_paths.is_none() || skill.local_paths.as_ref().unwrap().is_empty() {
            if let Some(local_path) = &skill.local_path {
                let path = PathBuf::from(local_path);
                if path.exists() {
                    if path.is_dir() {
                        std::fs::remove_dir_all(&path)
                            .context("无法删除技能目录，请检查文件是否被占用")?;
                    } else {
                        std::fs::remove_file(&path)
                            .context("无法删除技能文件，请检查文件是否被占用")?;
                    }
                }
            }
        }

        // 更新数据库
        skill.installed = false;
        skill.installed_at = None;
        skill.local_path = None;
        skill.local_paths = None;

        self.db.save_skill(&skill).context("更新数据库失败")?;

        log::info!("Skill uninstalled successfully: {}", skill.name);
        Ok(())
    }

    /// 卸载特定路径的技能
    pub fn uninstall_skill_path(&self, skill_id: &str, path_to_remove: &str) -> Result<()> {
        // 从数据库获取 skill
        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 删除指定路径的文件
        let path = PathBuf::from(path_to_remove);
        if path.exists() {
            if path.is_dir() {
                std::fs::remove_dir_all(&path).context("无法删除技能目录，请检查文件是否被占用")?;
            } else {
                std::fs::remove_file(&path).context("无法删除技能文件，请检查文件是否被占用")?;
            }
        }

        // 从 local_paths 中移除该路径
        if let Some(mut paths) = skill.local_paths.clone() {
            paths.retain(|p| p != path_to_remove);

            if paths.is_empty() {
                // 如果没有剩余路径,标记为未安装
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

        log::info!(
            "Skill path uninstalled: {} from {}",
            skill.name,
            path_to_remove
        );
        Ok(())
    }

    /// 获取所有 skills
    pub fn get_all_skills(&self) -> Result<Vec<Skill>> {
        self.db.get_skills()
    }

    /// 获取已安装的 skills
    pub fn get_installed_skills(&self) -> Result<Vec<Skill>> {
        let skills = self.db.get_skills()?;
        Ok(skills.into_iter().filter(|s| s.installed).collect())
    }

    /// 扫描本地 ~/.claude/skills/ 目录，导入未追踪的技能
    pub fn scan_local_skills(&self) -> Result<Vec<Skill>> {
        use std::collections::HashSet;

        let mut scanned_skills = Vec::new(); // 所有扫描到的技能
        let mut imported_skills = Vec::new(); // 新导入的技能（用于日志）

        // 获取当前数据库中的所有技能（用于去重和提取路径）
        let existing_skills = self.db.get_skills()?;

        // 1. 获取所有 unique 的 local_path 父目录
        let mut scan_dirs: HashSet<PathBuf> = HashSet::new();

        // 从已安装技能的 local_path 提取父目录
        for skill in &existing_skills {
            if let Some(local_path) = &skill.local_path {
                if let Some(parent) = PathBuf::from(local_path).parent() {
                    scan_dirs.insert(parent.to_path_buf());
                }
            }
        }

        // 2. 添加默认的用户目录（确保始终扫描）
        scan_dirs.insert(self.skills_dir.clone());

        log::info!("Will scan {} directories for local skills", scan_dirs.len());

        // 3. 扫描所有目录
        for scan_dir in scan_dirs {
            if !scan_dir.exists() {
                log::debug!("Skipping non-existent directory: {:?}", scan_dir);
                continue;
            }

            log::info!("Scanning directory: {:?}", scan_dir);

            // 遍历技能目录
            if let Ok(entries) = std::fs::read_dir(&scan_dir) {
                for entry in entries.flatten() {
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

                    // 读取 SKILL.md 内容
                    match std::fs::read_to_string(&skill_md_path) {
                        Ok(content) => {
                            // 计算 checksum
                            let checksum = self.scanner.calculate_checksum(content.as_bytes());

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

                            // 检查是否已存在（按 local_path 去重，避免目录不变但名称变化导致重复导入）
                            let local_path_str = path.to_string_lossy().to_string();
                            let existing_by_path = existing_skills
                                .iter()
                                .filter(|s| {
                                    s.local_path.as_deref() == Some(local_path_str.as_str())
                                })
                                .cloned()
                                .collect::<Vec<_>>();

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

                                // 更新 checksum（基于 SKILL.md 内容）
                                if existing_skill.checksum.as_deref() != Some(checksum.as_str()) {
                                    existing_skill.checksum = Some(checksum.clone());
                                }

                                // 仅对本地导入的技能（repository_url == local）更新 name/description/file_path
                                // 避免覆盖市场技能的元数据来源（仓库扫描/市场配置）
                                if existing_skill.repository_url == "local" {
                                    existing_skill.name = skill_name;
                                    existing_skill.description = skill_description;
                                    existing_skill.file_path = local_path_str.clone();
                                }

                                // 命中已有 local_path：刷新安全扫描信息，避免安全结果陈旧
                                let report = self.scanner.scan_directory_with_options(
                                    path.to_str().unwrap_or(""),
                                    &existing_skill.id,
                                    "zh",
                                    ScanOptions { skip_readme: true },
                                    None,
                                )?;

                                existing_skill.security_score = Some(report.score);
                                existing_skill.security_issues = Some(
                                    report
                                        .issues
                                        .iter()
                                        .map(|i| {
                                            let file_info = i
                                                .file_path
                                                .as_ref()
                                                .map(|f| format!("[{}] ", f))
                                                .unwrap_or_default();
                                            format!(
                                                "{}{:?}: {}",
                                                file_info, i.severity, i.description
                                            )
                                        })
                                        .collect(),
                                );
                                existing_skill.security_level = Some(match report.level {
                                    crate::models::security::SecurityLevel::Safe => {
                                        "Safe".to_string()
                                    }
                                    crate::models::security::SecurityLevel::Low => {
                                        "Low".to_string()
                                    }
                                    crate::models::security::SecurityLevel::Medium => {
                                        "Medium".to_string()
                                    }
                                    crate::models::security::SecurityLevel::High => {
                                        "High".to_string()
                                    }
                                    crate::models::security::SecurityLevel::Critical => {
                                        "Critical".to_string()
                                    }
                                });
                                existing_skill.scanned_at = Some(Utc::now());

                                self.db.save_skill(&existing_skill)?;
                                scanned_skills.push(existing_skill);
                                continue;
                            }

                            // 生成技能 ID
                            let skill_id = format!("local::{}", checksum[..16].to_string());

                            // 扫描整个技能目录
                            let report = self.scanner.scan_directory_with_options(
                                path.to_str().unwrap_or(""),
                                &skill_id,
                                "zh",
                                ScanOptions { skip_readme: true },
                                None,
                            )?;

                            log::info!(
                                "Scanned local skill '{}': score={}, files={:?}",
                                skill_name,
                                report.score,
                                report.scanned_files
                            );

                            // 创建 skill 对象（使用之前解析的元数据）
                            let local_path_str = path.to_string_lossy().to_string();
                            let skill = Skill {
                                id: skill_id,
                                name: skill_name,
                                description: skill_description,
                                repository_url: "local".to_string(),
                                repository_owner: Some("local".to_string()),
                                file_path: path.to_string_lossy().to_string(),
                                version: None,
                                author: None,
                                installed: true,
                                installed_at: Some(Utc::now()),
                                local_path: Some(local_path_str.clone()),
                                local_paths: Some(vec![local_path_str]),
                                checksum: Some(checksum),
                                security_score: Some(report.score),
                                security_issues: Some(
                                    report
                                        .issues
                                        .iter()
                                        .map(|i| {
                                            let file_info = i
                                                .file_path
                                                .as_ref()
                                                .map(|f| format!("[{}] ", f))
                                                .unwrap_or_default();
                                            format!(
                                                "{}{:?}: {}",
                                                file_info, i.severity, i.description
                                            )
                                        })
                                        .collect(),
                                ),
                                security_level: Some(match report.level {
                                    crate::models::security::SecurityLevel::Safe => {
                                        "Safe".to_string()
                                    }
                                    crate::models::security::SecurityLevel::Low => {
                                        "Low".to_string()
                                    }
                                    crate::models::security::SecurityLevel::Medium => {
                                        "Medium".to_string()
                                    }
                                    crate::models::security::SecurityLevel::High => {
                                        "High".to_string()
                                    }
                                    crate::models::security::SecurityLevel::Critical => {
                                        "Critical".to_string()
                                    }
                                }),
                                scanned_at: Some(Utc::now()),
                                installed_commit_sha: None,
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

        log::info!(
            "Scanned {} local skills, imported {} new skills",
            scanned_skills.len(),
            imported_skills.len()
        );
        Ok(scanned_skills)
    }

    /// 解析 SKILL.md 的 frontmatter
    fn parse_frontmatter(&self, content: &str) -> Result<(String, Option<String>)> {
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || lines[0] != "---" {
            anyhow::bail!("Invalid SKILL.md format: missing frontmatter");
        }

        // 找到第二个 "---"
        let end_index = lines
            .iter()
            .skip(1)
            .position(|&line| line == "---")
            .context("Invalid SKILL.md format: frontmatter not closed")?;

        // 提取 frontmatter 内容
        let frontmatter_lines = &lines[1..=end_index];
        let _frontmatter_str = frontmatter_lines.join("\n");

        // 简单的 YAML 解析（只提取 name 和 description）
        let mut name = String::new();
        let mut description: Option<String> = None;

        for line in frontmatter_lines {
            if let Some(stripped) = line.strip_prefix("name:") {
                name = stripped.trim().to_string();
            } else if let Some(stripped) = line.strip_prefix("description:") {
                description = Some(stripped.trim().to_string());
            }
        }

        if name.is_empty() {
            anyhow::bail!("Missing 'name' field in frontmatter");
        }

        Ok((name, description))
    }

    /// 从网络下载并安装技能（降级方案）
    async fn install_from_network(
        &self,
        skill: &crate::models::Skill,
        skill_dir: &PathBuf,
    ) -> Result<()> {
        let (owner, repo) = crate::models::Repository::from_github_url(&skill.repository_url)?;

        // 如果 file_path 是 "."，转换为空字符串以获取根目录内容
        let api_path = if skill.file_path == "." {
            ""
        } else {
            &skill.file_path
        };
        let skill_files = self
            .github
            .get_directory_files(&owner, &repo, api_path)
            .await
            .context("获取技能目录文件列表失败")?;

        log::info!("Found {} files in skill directory", skill_files.len());

        // 下载每个文件
        for file_info in &skill_files {
            if file_info.content_type != "file" {
                continue; // 跳过子目录
            }

            // 获取 download_url
            let download_url = file_info
                .download_url
                .as_ref()
                .context(format!("文件 {} 缺少下载链接", file_info.name))?;

            let file_content = self
                .github
                .download_file(download_url)
                .await
                .context(format!("下载文件失败: {}", file_info.name))?;

            // 写入文件到本地
            let local_file_path = skill_dir.join(&file_info.name);
            std::fs::write(&local_file_path, file_content)
                .context(format!("无法写入文件: {}", file_info.name))?;

            log::info!("Saved file: {}", file_info.name);
        }

        Ok(())
    }

    /// 检测本地文件是否被修改（与缓存中的版本比较）
    fn detect_local_modifications(
        &self,
        installed_dir: &PathBuf,
        cached_dir: &PathBuf,
    ) -> Result<Vec<String>> {
        use std::fs;

        let mut modified_files = Vec::new();

        // 遍历已安装目录中的所有文件
        for entry in walkdir::WalkDir::new(installed_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let installed_file = entry.path();

                // 计算相对路径
                let relative_path = installed_file
                    .strip_prefix(installed_dir)
                    .context("无法计算相对路径")?;

                // 对应的缓存文件路径
                let cached_file = cached_dir.join(relative_path);

                // 如果缓存中没有该文件，说明是用户新增的
                if !cached_file.exists() {
                    modified_files.push(format!("新增: {}", relative_path.display()));
                    continue;
                }

                // 比较文件内容
                let installed_content = fs::read(installed_file)?;
                let cached_content = fs::read(&cached_file)?;

                if installed_content != cached_content {
                    modified_files.push(format!("修改: {}", relative_path.display()));
                }
            }
        }

        Ok(modified_files)
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
            .context("未找到该技能")?;

        if !skill.installed {
            anyhow::bail!("该技能尚未安装，无法更新");
        }

        // 获取仓库记录
        let repositories = self.db.get_repositories()?;
        let repo = repositories
            .iter()
            .find(|r| r.url == skill.repository_url)
            .context("未找到对应的仓库记录")?
            .clone();

        // 重新下载仓库到新的临时缓存（staging）
        log::info!("下载最新版本到 staging 目录");
        let (owner, repo_name) = crate::models::Repository::from_github_url(&skill.repository_url)?;

        let staging_base_dir = dirs::cache_dir()
            .context("无法获取系统缓存目录")?
            .join("agent-skills-guard")
            .join("staging");

        // 清理旧的 staging 目录（如果存在）
        let staging_repo_dir = staging_base_dir.join(format!("{}_{}", owner, repo_name));
        if staging_repo_dir.exists() {
            std::fs::remove_dir_all(&staging_repo_dir)?;
        }

        // 下载最新版本
        let (extract_dir, new_commit_sha) = self
            .github
            .download_repository_archive(&owner, &repo_name, &staging_base_dir)
            .await
            .context("下载最新版本失败")?;

        log::info!("下载完成，最新 commit: {}", new_commit_sha);

        // 定位 staging 中的技能目录
        let staging_skill_dir =
            self.locate_skill_in_cache(extract_dir.as_path(), &skill.file_path)?;

        // 扫描最新版本
        let scan_report = self.scanner.scan_directory_with_options(
            staging_skill_dir.to_str().context("技能目录路径无效")?,
            &skill.id,
            locale,
            ScanOptions { skip_readme: true },
            None,
        )?;

        log::info!(
            "Security scan completed: score={}, scanned {} files",
            scan_report.score,
            scan_report.scanned_files.len()
        );

        // 检测本地修改
        let modified_files = if let Some(local_path) = &skill.local_path {
            let installed_dir = PathBuf::from(local_path);
            if installed_dir.exists() {
                // 获取当前缓存中的版本（用于比较）
                if let Some(cache_path) = &repo.cache_path {
                    let cache_path_buf = PathBuf::from(cache_path);
                    if cache_path_buf.exists() {
                        match self.locate_skill_in_cache(cache_path_buf.as_path(), &skill.file_path)
                        {
                            Ok(cached_skill_dir) => {
                                self.detect_local_modifications(&installed_dir, &cached_skill_dir)?
                            }
                            Err(e) => {
                                log::warn!("无法定位缓存中的技能目录: {}", e);
                                Vec::new()
                            }
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        log::info!("检测到 {} 个本地修改", modified_files.len());

        // 保存 staging 信息到数据库（临时）
        // 我们使用一个特殊的字段来标记这是 staging 路径
        let mut skill_update = skill.clone();
        skill_update.local_path = Some(format!(
            "__staging__:{}",
            staging_skill_dir.to_string_lossy()
        ));

        self.db.save_skill(&skill_update)?;

        Ok((scan_report, modified_files))
    }

    /// 确认技能更新：从 staging 写入到安装目录，并在缓存目录保留备份
    pub fn confirm_skill_update(&self, skill_id: &str, force_overwrite: bool) -> Result<()> {
        use anyhow::Context;
        use std::{io, thread, time::Duration};

        log::info!("Confirming update for skill: {}", skill_id);

        let mut skill = self
            .db
            .get_skills()?
            .into_iter()
            .find(|s| s.id == skill_id)
            .context("未找到该技能")?;

        // 获取 staging 路径
        let staging_marker = skill.local_path.as_ref().context("技能尚未准备更新")?;

        if !staging_marker.starts_with("__staging__:") {
            anyhow::bail!("技能尚未准备更新，请先调用 prepare_skill_update");
        }

        let staging_path_str = &staging_marker[12..]; // 去掉 "__staging__:" 前缀
        let staging_dir = PathBuf::from(staging_path_str);

        if !staging_dir.exists() {
            anyhow::bail!("Staging 目录不存在");
        }

        // 获取原安装路径（从 local_paths）
        let install_paths = skill.local_paths.as_ref().context("无法获取安装路径")?;

        if install_paths.is_empty() {
            anyhow::bail!("技能没有有效的安装路径");
        }

        // 使用第一个路径作为目标（通常只有一个）
        let target_install_dir = PathBuf::from(&install_paths[0]);

        #[derive(Debug)]
        enum BackupDir {
            Renamed(PathBuf),
            Copied(PathBuf),
        }

        fn is_retryable_rename_error(err: &io::Error) -> bool {
            if err.kind() == io::ErrorKind::PermissionDenied {
                return true;
            }

            matches!(err.raw_os_error(), Some(5 | 32 | 33))
        }

        fn rename_with_retry(from: &PathBuf, to: &PathBuf) -> io::Result<()> {
            let mut last_err: Option<io::Error> = None;
            let attempts = 6usize;
            let delay = Duration::from_millis(250);

            for attempt in 0..attempts {
                match std::fs::rename(from, to) {
                    Ok(()) => return Ok(()),
                    Err(err) => {
                        let retryable = is_retryable_rename_error(&err);
                        let is_last = attempt + 1 >= attempts;
                        last_err = Some(err);
                        if retryable && !is_last {
                            thread::sleep(delay);
                            continue;
                        }
                        break;
                    }
                }
            }

            Err(last_err.unwrap_or_else(|| {
                io::Error::new(io::ErrorKind::Other, "rename_with_retry failed")
            }))
        }

        // 创建备份（如果目录存在）：优先移动到缓存目录；若移动失败则复制到缓存目录
        let backup_dir = if target_install_dir.exists() {
            let dir_name = target_install_dir
                .file_name()
                .context("无效的目录名")?
                .to_string_lossy();
            let backup_root = dirs::cache_dir()
                .context("无法获取系统缓存目录")?
                .join("agent-skills-guard")
                .join("skill-backups");

            std::fs::create_dir_all(&backup_root)
                .context(format!("无法创建备份缓存目录: {:?}", backup_root))?;

            let mut backup_path = backup_root.join(format!("{}.bak", dir_name));

            if backup_path.exists() {
                match std::fs::remove_dir_all(&backup_path) {
                    Ok(()) => {}
                    Err(remove_err) => {
                        if !force_overwrite {
                            return Err(anyhow::anyhow!(format!(
                                "无法删除旧备份目录（缓存目录）: {:?}\n错误: {}\n\n请检查该目录是否被其他程序占用",
                                backup_path, remove_err
                            )));
                        }

                        // 强制覆盖时，为了不中断流程，改用一个唯一的备份目录名
                        let epoch_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis();
                        backup_path = backup_root.join(format!("{}.bak-{}", dir_name, epoch_ms));
                        let _ = std::fs::remove_dir_all(&backup_path);
                    }
                }
            }

            // 尝试移动：移动成功意味着我们可以“干净地”写入新版本（更接近原子替换）
            match rename_with_retry(&target_install_dir, &backup_path) {
                Ok(()) => {
                    log::info!("创建备份(移动到缓存): {:?}", backup_path);
                    Some(BackupDir::Renamed(backup_path))
                }
                Err(move_err) => {
                    log::warn!(
                        "无法移动技能目录到缓存备份（将改用复制备份 + 原地覆盖）: {}",
                        move_err
                    );

                    match self.copy_directory(&target_install_dir, &backup_path) {
                        Ok(()) => {
                            log::info!("创建备份(复制到缓存): {:?}", backup_path);
                            Some(BackupDir::Copied(backup_path))
                        }
                        Err(copy_err) => {
                            if !force_overwrite {
                                return Err(anyhow::anyhow!(format!(
                                    "无法为更新创建备份（缓存目录）\n目标: {:?}\n备份: {:?}\n\n复制备份错误: {}\n\n提示：你可以关闭正在使用该技能的程序后重试；或勾选“强制覆盖本地修改”继续（将无法保证可回滚）。",
                                    target_install_dir, backup_path, copy_err
                                )));
                            }

                            log::warn!(
                                "创建备份(复制到缓存)失败，将在无备份情况下继续: {}",
                                copy_err
                            );
                            None
                        }
                    }
                }
            }
        } else {
            None
        };

        // 确保目标父目录存在
        std::fs::create_dir_all(&target_install_dir.parent().context("无效的安装路径")?)?;

        // 如果前面“移动备份”成功，目标目录已不存在；先创建一个干净目录
        if !target_install_dir.exists() {
            std::fs::create_dir_all(&target_install_dir)
                .context(format!("无法创建目标目录: {:?}", target_install_dir))?;
        } else if force_overwrite {
            // 强制覆盖时，尽量清空旧目录以避免遗留文件
            if let Err(clear_err) = std::fs::remove_dir_all(&target_install_dir) {
                log::warn!(
                    "无法清空旧技能目录，将尝试直接覆盖写入（可能保留部分旧文件）: {}",
                    clear_err
                );
            } else {
                std::fs::create_dir_all(&target_install_dir)
                    .context(format!("无法重建目标目录: {:?}", target_install_dir))?;
            }
        }

        match self.copy_directory(&staging_dir, &target_install_dir) {
            Ok(_) => {
                log::info!("成功更新技能到: {:?}", target_install_dir);

                // 备份保留在缓存目录，便于必要时人工回滚；下一次更新会覆盖旧备份

                // 更新数据库：恢复 local_path，更新 installed_commit_sha
                skill.local_path = Some(target_install_dir.to_string_lossy().to_string());

                // 从 staging 路径推导出 extracted 目录并提取 commit SHA
                // - staging_dir 指向 skill 目录（可能是仓库根目录或其子目录）
                // - extracted_dir 是 {cache}/.../extracted/，其下第一层目录名为 {owner}-{repo}-{sha}
                let extract_dir = {
                    let mut repo_root = staging_dir.clone();
                    if skill.file_path != "." {
                        let components_count = std::path::Path::new(&skill.file_path)
                            .components()
                            .filter(|c| matches!(c, std::path::Component::Normal(_)))
                            .count();

                        for _ in 0..components_count {
                            repo_root = repo_root
                                .parent()
                                .context("无效的 staging 路径：无法定位仓库根目录")?
                                .to_path_buf();
                        }
                    }

                    repo_root
                        .parent()
                        .context("无效的 staging 路径：无法定位 extracted 目录")?
                        .to_path_buf()
                };

                match self.github.extract_commit_sha_from_cache(&extract_dir) {
                    Ok(new_sha) => {
                        skill.installed_commit_sha = Some(new_sha.clone());
                        log::info!("更新 installed_commit_sha");

                        // 将 staging 下载的版本提升为“仓库缓存基线”，避免后续把已更新内容误判为“本地修改”
                        if let Ok((owner, repo_name)) =
                            crate::models::Repository::from_github_url(&skill.repository_url)
                        {
                            if let Some(cache_base_dir) = dirs::cache_dir() {
                                let repositories_base_dir = cache_base_dir
                                    .join("agent-skills-guard")
                                    .join("repositories");
                                let repo_cache_dir =
                                    repositories_base_dir.join(format!("{}_{}", owner, repo_name));
                                let extracted_dest = repo_cache_dir.join("extracted");

                                if let Err(e) = std::fs::create_dir_all(&repo_cache_dir) {
                                    log::warn!("无法创建仓库缓存目录，将跳过缓存同步: {}", e);
                                } else {
                                    if extracted_dest.exists() {
                                        let _ = std::fs::remove_dir_all(&extracted_dest);
                                    }

                                    match rename_with_retry(&extract_dir, &extracted_dest) {
                                        Ok(()) => {
                                            log::info!(
                                                "已同步仓库缓存(移动): {:?}",
                                                extracted_dest
                                            );
                                        }
                                        Err(rename_err) => {
                                            log::warn!(
                                                "无法移动 staging 缓存到仓库缓存，将尝试复制: {}",
                                                rename_err
                                            );
                                            if let Err(copy_err) =
                                                self.copy_directory(&extract_dir, &extracted_dest)
                                            {
                                                log::warn!("同步仓库缓存(复制)失败: {}", copy_err);
                                            } else {
                                                log::info!(
                                                    "已同步仓库缓存(复制): {:?}",
                                                    extracted_dest
                                                );
                                            }
                                        }
                                    }

                                    if extracted_dest.exists() {
                                        if let Ok(repositories) = self.db.get_repositories() {
                                            if let Some(repo) = repositories
                                                .iter()
                                                .find(|r| r.url == skill.repository_url)
                                            {
                                                let cache_path_str =
                                                    extracted_dest.to_string_lossy().to_string();
                                                if let Err(e) = self.db.update_repository_cache(
                                                    &repo.id,
                                                    &cache_path_str,
                                                    Utc::now(),
                                                    Some(&new_sha),
                                                ) {
                                                    log::warn!("更新仓库缓存信息失败: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("无法提取新的 commit SHA: {}", e);
                    }
                }

                skill.installed_at = Some(Utc::now());
                self.db.save_skill(&skill)?;

                log::info!("技能更新确认完成: {}", skill.name);
                Ok(())
            }
            Err(e) => {
                // 恢复备份
                if let Some(backup) = backup_dir {
                    if target_install_dir.exists() {
                        let _ = std::fs::remove_dir_all(&target_install_dir);
                    }

                    match backup {
                        BackupDir::Renamed(p) => {
                            let _ = std::fs::rename(&p, &target_install_dir);
                            log::warn!("更新失败，已恢复备份(重命名): {:?}", p);
                        }
                        BackupDir::Copied(p) => {
                            let _ = self.copy_directory(&p, &target_install_dir);
                            log::warn!("更新失败，已恢复备份(复制): {:?}", p);
                        }
                    }
                }
                Err(e)
            }
        }
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
            .context("未找到该技能")?;

        // 获取 staging 路径
        let staging_marker = skill.local_path.as_ref().context("技能尚未准备更新")?;

        if !staging_marker.starts_with("__staging__:") {
            log::warn!("技能没有处于更新准备状态");
            return Ok(());
        }

        let staging_path_str = &staging_marker[12..];
        let staging_dir = PathBuf::from(staging_path_str);

        // 删除 staging 目录（整个 staging repo 目录）
        if let Some(parent) = staging_dir.parent() {
            if let Some(repo_dir) = parent.parent() {
                if repo_dir.exists() {
                    std::fs::remove_dir_all(repo_dir)?;
                    log::info!("已删除 staging 目录: {:?}", repo_dir);
                }
            }
        }

        // 恢复数据库中的 local_path
        if let Some(local_paths) = &skill.local_paths {
            if !local_paths.is_empty() {
                skill.local_path = Some(local_paths[0].clone());
            } else {
                skill.local_path = None;
            }
        } else {
            skill.local_path = None;
        }

        self.db.save_skill(&skill)?;

        log::info!("技能更新已取消: {}", skill.name);
        Ok(())
    }

    /// 递归复制目录
    fn copy_directory(&self, src: &PathBuf, dst: &PathBuf) -> Result<()> {
        use std::fs;

        log::info!("复制目录: {:?} -> {:?}", src, dst);

        // 确保目标目录存在
        if !dst.exists() {
            fs::create_dir_all(dst).context(format!("无法创建目标目录: {:?}", dst))?;
            log::debug!("创建目标目录: {:?}", dst);
        }

        // 遍历源目录
        for entry in fs::read_dir(src).context(format!("无法读取源目录: {:?}", src))? {
            let entry = entry.context(format!("读取目录项失败: {:?}", src))?;
            let file_type = entry
                .file_type()
                .context(format!("获取文件类型失败: {:?}", entry.path()))?;
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);

            if file_type.is_dir() {
                // 递归复制子目录
                log::debug!("复制子目录: {:?}", file_name);
                self.copy_directory(&src_path, &dst_path)?;
            } else if file_type.is_file() {
                // 确保目标文件的父目录存在
                if let Some(parent) = dst_path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)
                            .context(format!("无法创建文件父目录: {:?}", parent))?;
                    }
                }

                // 复制文件
                match fs::copy(&src_path, &dst_path) {
                    Ok(bytes) => {
                        log::debug!("已复制文件: {:?} ({} bytes)", file_name, bytes);
                    }
                    Err(e) => {
                        // 提供详细的错误信息
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

        log::info!("目录复制完成: {:?}", dst);
        Ok(())
    }
}
