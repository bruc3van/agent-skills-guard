use crate::models::{GitHubContent, Repository, Skill};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::fs::{self, File};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use zip::ZipArchive;

/// GitHub Commit API 响应
#[derive(Debug, Deserialize)]
struct GitHubCommit {
    sha: String,
    #[allow(dead_code)]
    commit: GitHubCommitDetail,
}

#[derive(Debug, Deserialize)]
struct GitHubCommitDetail {
    #[allow(dead_code)]
    author: GitHubCommitAuthor,
    #[allow(dead_code)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct GitHubCommitAuthor {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    date: String,
}

/// SKILL.md 文件的 frontmatter
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: Option<String>,
}

pub struct GitHubService {
    client: Client,
    api_base: String,
}

impl GitHubService {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("agent-skills-guard")
                .timeout(std::time::Duration::from_secs(30)) // 30秒超时
                .connect_timeout(std::time::Duration::from_secs(10)) // 10秒连接超时
                .build()
                .unwrap(),
            api_base: "https://api.github.com".to_string(),
        }
    }

    /// 扫描仓库中的 skills
    pub async fn scan_repository(&self, repo: &Repository) -> Result<Vec<Skill>> {
        let (owner, repo_name) = Repository::from_github_url(&repo.url)?;
        let mut skills = Vec::new();

        // 获取仓库根目录内容
        let contents = self
            .fetch_directory_contents(&owner, &repo_name, "")
            .await?;

        for item in contents {
            if item.content_type == "dir" {
                // 检查文件夹是否为 skill（包含 SKILL.md）
                if self
                    .is_skill_directory(&owner, &repo_name, &item.path)
                    .await?
                {
                    // 获取 skill 的元数据（name 和 description）
                    let (name, description) = match self
                        .fetch_skill_metadata(&owner, &repo_name, &item.path)
                        .await
                    {
                        Ok(metadata) => metadata,
                        Err(e) => {
                            log::warn!(
                                "Failed to fetch metadata for {}: {}, using fallback",
                                item.path,
                                e
                            );
                            (item.name.clone(), None)
                        }
                    };

                    // 如果路径为空（在根目录），设置为 "."
                    let file_path = if item.path.trim().is_empty() {
                        log::info!("技能 {} 位于仓库根目录，设置 file_path 为 '.'", name);
                        ".".to_string()
                    } else {
                        item.path.clone()
                    };

                    let mut skill = Skill::new(name, repo.url.clone(), file_path);
                    skill.description = description;
                    skills.push(skill);
                } else if repo.scan_subdirs {
                    // 递归扫描子目录
                    match self
                        .scan_directory(&owner, &repo_name, &item.path, &repo.url)
                        .await
                    {
                        Ok(mut sub_skills) => skills.append(&mut sub_skills),
                        Err(e) => log::warn!("Failed to scan subdirectory {}: {}", item.path, e),
                    }
                }
            }
        }

        Ok(skills)
    }

    /// 递归扫描目录
    fn scan_directory<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        path: &'a str,
        repo_url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Skill>>> + Send + 'a>> {
        Box::pin(async move {
            let mut skills = Vec::new();
            let contents = self.fetch_directory_contents(owner, repo, path).await?;

            for item in contents {
                if item.content_type == "dir" {
                    // 检查文件夹是否为 skill（包含 SKILL.md）
                    if self.is_skill_directory(owner, repo, &item.path).await? {
                        // 获取 skill 的元数据（name 和 description）
                        let (name, description) =
                            match self.fetch_skill_metadata(owner, repo, &item.path).await {
                                Ok(metadata) => metadata,
                                Err(e) => {
                                    log::warn!(
                                        "Failed to fetch metadata for {}: {}, using fallback",
                                        item.path,
                                        e
                                    );
                                    (item.name.clone(), None)
                                }
                            };

                        // 如果路径为空（在根目录），设置为 "."
                        let file_path = if item.path.trim().is_empty() {
                            log::info!("技能 {} 位于仓库根目录，设置 file_path 为 '.'", name);
                            ".".to_string()
                        } else {
                            item.path.clone()
                        };

                        let mut skill = Skill::new(name, repo_url.to_string(), file_path);
                        skill.description = description;
                        skills.push(skill);
                    } else if path.split('/').count() < 5 {
                        // 递归扫描（限制深度避免无限递归）
                        match self.scan_directory(owner, repo, &item.path, repo_url).await {
                            Ok(mut sub_skills) => skills.append(&mut sub_skills),
                            Err(e) => {
                                log::warn!("Failed to scan subdirectory {}: {}", item.path, e)
                            }
                        }
                    }
                }
            }

            Ok(skills)
        })
    }

    /// 获取目录内容
    async fn fetch_directory_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> Result<Vec<GitHubContent>> {
        let url = if path.is_empty() {
            format!("{}/repos/{}/{}/contents", self.api_base, owner, repo)
        } else {
            format!(
                "{}/repos/{}/{}/contents/{}",
                self.api_base, owner, repo, path
            )
        };

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("网络请求失败，请检查您的网络连接")?;

        let status = response.status();

        // 处理不同的 HTTP 错误
        if !status.is_success() {
            match status.as_u16() {
                403 => {
                    // 检查是否是 API 限流
                    if let Some(remaining) = response.headers().get("x-ratelimit-remaining") {
                        if remaining == "0" {
                            if let Some(reset) = response.headers().get("x-ratelimit-reset") {
                                // 将 Unix 时间戳转换为可读格式
                                if let Ok(reset_str) = reset.to_str() {
                                    if let Ok(reset_timestamp) = reset_str.parse::<i64>() {
                                        let now = std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs()
                                            as i64;
                                        let wait_seconds = reset_timestamp - now;

                                        if wait_seconds > 0 {
                                            let wait_minutes = (wait_seconds + 59) / 60; // 向上取整
                                            anyhow::bail!(
                                                "GitHub API 速率限制已达上限，请等待约 {} 分钟后重试。\n\n提示：未认证的请求限制为每小时60次，认证后可提升至5000次/小时。",
                                                wait_minutes
                                            );
                                        }
                                    }
                                }
                            }
                            anyhow::bail!("GitHub API 速率限制已达上限，请稍后重试（约1小时后）");
                        }
                    }
                    anyhow::bail!("无权限访问该仓库，请检查仓库是否为私有仓库");
                }
                404 => {
                    anyhow::bail!("仓库或路径不存在: {}/{}", owner, repo);
                }
                401 => {
                    anyhow::bail!("未授权访问，请配置 GitHub Token");
                }
                500..=599 => {
                    anyhow::bail!("GitHub 服务器错误，请稍后重试");
                }
                _ => {
                    anyhow::bail!("GitHub API 返回错误: {}", status);
                }
            }
        }

        let contents: Vec<GitHubContent> = response
            .json()
            .await
            .context("解析 GitHub 响应失败，数据格式可能不正确")?;

        Ok(contents)
    }

    /// 下载文件内容
    pub async fn download_file(&self, download_url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(download_url)
            .send()
            .await
            .context("网络请求失败，无法下载文件")?;

        let status = response.status();

        if !status.is_success() {
            match status.as_u16() {
                403 => {
                    if let Some(remaining) = response.headers().get("x-ratelimit-remaining") {
                        if remaining == "0" {
                            anyhow::bail!("GitHub API 速率限制已达上限，请稍后重试");
                        }
                    }
                    anyhow::bail!("无权限访问该文件");
                }
                404 => {
                    anyhow::bail!("文件不存在: {}", download_url);
                }
                _ => {
                    anyhow::bail!("下载文件失败: {}", status);
                }
            }
        }

        let bytes = response.bytes().await.context("读取文件内容失败")?;

        Ok(bytes.to_vec())
    }

    /// 判断文件夹是否为 skill（包含 SKILL.md）
    async fn is_skill_directory(&self, owner: &str, repo: &str, path: &str) -> Result<bool> {
        // 获取文件夹内容
        match self.fetch_directory_contents(owner, repo, path).await {
            Ok(contents) => {
                // 检查是否包含 SKILL.md 文件
                Ok(contents.iter().any(|item| {
                    item.content_type == "file" && item.name.to_uppercase() == "SKILL.MD"
                }))
            }
            Err(e) => {
                log::warn!("Failed to check directory {}: {}", path, e);
                Ok(false)
            }
        }
    }

    /// 下载并解析 SKILL.md 的 frontmatter
    pub async fn fetch_skill_metadata(
        &self,
        owner: &str,
        repo: &str,
        skill_path: &str,
    ) -> Result<(String, Option<String>)> {
        // 尝试多个分支获取 SKILL.md
        let branches = ["main", "master"];
        let mut last_error = None;

        for branch in branches.iter() {
            let download_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}/SKILL.md",
                owner, repo, branch, skill_path
            );

            log::info!("尝试从分支 {} 获取 SKILL.md: {}", branch, download_url);

            match self.download_file(&download_url).await {
                Ok(content) => match String::from_utf8(content) {
                    Ok(content_str) => {
                        log::info!("成功从分支 {} 获取 SKILL.md", branch);
                        return self.parse_skill_frontmatter(&content_str);
                    }
                    Err(e) => {
                        last_error =
                            Some(anyhow::anyhow!("Failed to decode SKILL.md as UTF-8: {}", e));
                        continue;
                    }
                },
                Err(e) => {
                    log::info!("分支 {} 不存在或获取失败: {}", branch, e);
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("所有分支均无法获取 SKILL.md")))
    }

    /// 解析 SKILL.md 的 frontmatter
    pub fn parse_skill_frontmatter(&self, content: &str) -> Result<(String, Option<String>)> {
        // 查找 frontmatter 的边界（--- ... ---）
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

        // 提取 frontmatter 内容（跳过第一个 "---"）
        let frontmatter_lines = &lines[1..=end_index];
        let frontmatter_str = frontmatter_lines.join("\n");

        // 解析 YAML
        let frontmatter: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)
            .context("Failed to parse SKILL.md frontmatter as YAML")?;

        Ok((frontmatter.name, frontmatter.description))
    }

    /// 获取目录下的所有文件（不递归）
    pub async fn get_directory_files(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
    ) -> Result<Vec<GitHubContent>> {
        let contents = self.fetch_directory_contents(owner, repo, path).await?;

        // 只返回文件，过滤掉子目录
        let files: Vec<GitHubContent> = contents
            .into_iter()
            .filter(|item| item.content_type == "file")
            .collect();

        Ok(files)
    }

    /// 下载仓库压缩包并解压到本地缓存
    /// 返回值：(extract_dir, commit_sha)
    pub async fn download_repository_archive(
        &self,
        owner: &str,
        repo: &str,
        cache_base_dir: &Path,
    ) -> Result<(PathBuf, String)> {
        // 1. 创建仓库专属缓存目录
        let repo_cache_dir = cache_base_dir.join(format!("{}_{}", owner, repo));
        fs::create_dir_all(&repo_cache_dir).context("无法创建缓存目录")?;

        // 2. 尝试下载压缩包（先尝试 main，如果 404 则尝试 master）
        let branches = ["main", "master"];
        let mut last_error = None;
        let mut response = None;

        for branch in branches.iter() {
            let url = format!(
                "{}/repos/{}/{}/zipball/{}",
                self.api_base, owner, repo, branch
            );
            log::info!("正在尝试下载仓库压缩包 (分支: {}): {}", branch, url);

            match self.client.get(&url).send().await {
                Ok(resp) => {
                    // 检查API限流
                    if let Err(e) = self.check_rate_limit(&resp) {
                        return Err(e);
                    }

                    if resp.status().is_success() {
                        log::info!("成功找到分支: {}", branch);
                        response = Some(resp);
                        break;
                    } else if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        log::info!("分支 {} 不存在，尝试下一个分支", branch);
                        last_error = Some(anyhow::anyhow!("分支 {} 不存在", branch));
                        continue;
                    } else {
                        last_error =
                            Some(anyhow::anyhow!("下载失败，HTTP状态码: {}", resp.status()));
                        continue;
                    }
                }
                Err(e) => {
                    log::warn!("请求分支 {} 时发生错误: {}", branch, e);
                    last_error = Some(anyhow::anyhow!("请求失败: {}", e));
                    continue;
                }
            }
        }

        let response = response
            .ok_or_else(|| last_error.unwrap_or_else(|| anyhow::anyhow!("所有分支均下载失败")))?;

        // 3. 保存压缩包到本地
        let archive_path = repo_cache_dir.join("archive.zip");
        let bytes = response.bytes().await.context("读取压缩包内容失败")?;

        let mut file = File::create(&archive_path).context("无法创建压缩包文件")?;
        file.write_all(&bytes).context("写入压缩包失败")?;

        log::info!(
            "压缩包已保存: {:?}, 大小: {} bytes",
            archive_path,
            bytes.len()
        );

        // 4. 解压缩
        let extract_dir = repo_cache_dir.join("extracted");
        self.extract_zip(&archive_path, &extract_dir)
            .context("解压缩失败")?;

        log::info!("解压完成: {:?}", extract_dir);

        // 5. 提取 commit SHA（从解压后的目录名）
        let commit_sha = self
            .extract_commit_sha_from_cache(&extract_dir)
            .context("无法提取 commit SHA")?;

        log::info!("提取到 commit SHA: {}", commit_sha);

        Ok((extract_dir, commit_sha))
    }

    /// 解压zip文件
    fn extract_zip(&self, archive_path: &Path, extract_dir: &Path) -> Result<()> {
        let file = File::open(archive_path).context("无法打开压缩包")?;

        let mut archive = ZipArchive::new(file).context("无法读取ZIP文件")?;

        log::info!("正在解压 {} 个文件...", archive.len());

        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .context(format!("无法读取ZIP条目 {}", i))?;

            // GitHub的zipball会在根目录包含一个 {owner}-{repo}-{commit}/ 的文件夹
            // 我们需要提取这个路径
            let outpath = match file.enclosed_name() {
                Some(path) => extract_dir.join(path),
                None => continue,
            };

            if file.is_dir() {
                fs::create_dir_all(&outpath).context(format!("无法创建目录: {:?}", outpath))?;
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent).context(format!("无法创建父目录: {:?}", parent))?;
                }

                let mut outfile =
                    File::create(&outpath).context(format!("无法创建文件: {:?}", outpath))?;

                std::io::copy(&mut file, &mut outfile)
                    .context(format!("无法写入文件: {:?}", outpath))?;
            }
        }

        Ok(())
    }

    /// 检查GitHub API限流状态
    fn check_rate_limit(&self, response: &reqwest::Response) -> Result<()> {
        if let Some(remaining) = response.headers().get("x-ratelimit-remaining") {
            if let Ok(remaining_str) = remaining.to_str() {
                log::debug!("GitHub API剩余配额: {}", remaining_str);

                if remaining_str == "0" {
                    if let Some(reset) = response.headers().get("x-ratelimit-reset") {
                        if let Ok(reset_str) = reset.to_str() {
                            if let Ok(reset_timestamp) = reset_str.parse::<i64>() {
                                let now = chrono::Utc::now().timestamp();
                                let wait_seconds = reset_timestamp - now;
                                let wait_minutes = (wait_seconds + 59) / 60;

                                return Err(anyhow::anyhow!(
                                    "GitHub API 速率限制已达上限，请等待约 {} 分钟后重试。\n\n提示：未认证的请求限制为每小时60次，认证后可提升至5000次/小时。",
                                    wait_minutes
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 从本地缓存扫描skills（不需要API请求）
    pub fn scan_cached_repository(
        &self,
        cache_path: &Path,
        repo_url: &str,
        scan_subdirs: bool,
    ) -> Result<Vec<Skill>> {
        use walkdir::WalkDir;

        let mut skills = Vec::new();
        let max_depth = if scan_subdirs { 10 } else { 2 };

        log::info!(
            "开始扫描本地缓存: {:?}, scan_subdirs: {}",
            cache_path,
            scan_subdirs
        );

        // GitHub zipball的根目录是 {owner}-{repo}-{commit}/
        // 需要找到这个根目录
        let root_dir = self.find_repo_root(cache_path)?;

        log::info!("找到仓库根目录: {:?}", root_dir);

        // 遍历本地文件系统
        for entry in WalkDir::new(&root_dir)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_dir() {
                // 检查是否包含SKILL.md
                let skill_md_path = entry.path().join("SKILL.md");
                if skill_md_path.exists() {
                    log::info!("发现skill: {:?}", entry.path());

                    // 读取并解析SKILL.md
                    match self.parse_skill_from_file(
                        &skill_md_path,
                        entry.path(),
                        &root_dir,
                        repo_url,
                    ) {
                        Ok(skill) => skills.push(skill),
                        Err(e) => log::warn!("解析skill失败 {:?}: {}", entry.path(), e),
                    }
                }
            }
        }

        log::info!("本地扫描完成，发现 {} 个skills", skills.len());

        Ok(skills)
    }

    /// 找到GitHub zipball解压后的根目录
    fn find_repo_root(&self, extract_dir: &Path) -> Result<PathBuf> {
        // GitHub zipball解压后会有一个 {owner}-{repo}-{commit}/ 目录
        // 我们需要找到这个目录
        for entry in fs::read_dir(extract_dir).context("无法读取解压目录")? {
            let entry = entry.context("无法读取目录条目")?;
            if entry.file_type()?.is_dir() {
                return Ok(entry.path());
            }
        }

        Err(anyhow::anyhow!("未找到仓库根目录"))
    }

    /// 从解压后的缓存目录中提取 commit SHA
    /// GitHub zipball 解压后的目录名格式：{owner}-{repo}-{commit_sha}
    pub fn extract_commit_sha_from_cache(&self, extract_dir: &Path) -> Result<String> {
        for entry in fs::read_dir(extract_dir).context("无法读取解压目录")? {
            let entry = entry.context("无法读取目录条目")?;
            if entry.file_type()?.is_dir() {
                // 获取目录名，格式为 {owner}-{repo}-{commit_sha}
                if let Some(dir_name) = entry.file_name().to_str() {
                    // 提取最后一个 `-` 之后的部分作为 commit SHA
                    if let Some(last_dash) = dir_name.rfind('-') {
                        let commit_sha = &dir_name[last_dash + 1..];
                        // 验证是否为合法的 SHA（至少 7 位十六进制字符）
                        if commit_sha.len() >= 7
                            && commit_sha.chars().all(|c| c.is_ascii_hexdigit())
                        {
                            return Ok(commit_sha.to_string());
                        }
                    }
                }
            }
        }

        Err(anyhow::anyhow!("无法从目录名提取 commit SHA"))
    }

    /// 从本地SKILL.md文件解析skill信息
    fn parse_skill_from_file(
        &self,
        skill_md_path: &Path,
        skill_dir: &Path,
        repo_root: &Path,
        repo_url: &str,
    ) -> Result<Skill> {
        // 读取SKILL.md内容
        let content = fs::read_to_string(skill_md_path).context("无法读取SKILL.md")?;

        // 解析frontmatter获取name和description
        let (name, description) = self.parse_skill_frontmatter(&content)?;

        // 计算相对于仓库根目录的路径
        let relative_path = skill_dir
            .strip_prefix(repo_root)
            .context("无法计算相对路径")?;

        let mut file_path = relative_path.to_string_lossy().to_string();

        // 如果 file_path 为空（SKILL.md 在仓库根目录），设置为 "."
        if file_path.trim().is_empty() {
            log::info!("技能位于仓库根目录，设置 file_path 为 '.'");
            file_path = ".".to_string();
        }

        // 计算checksum
        let checksum = self.calculate_checksum(&content);

        let mut skill = Skill::new(name, repo_url.to_string(), file_path);
        skill.description = description;
        skill.checksum = Some(checksum);

        Ok(skill)
    }

    /// 计算文件内容的SHA256 checksum
    fn calculate_checksum(&self, content: &str) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();

        hex::encode(result)
    }

    /// 检查技能是否有更新
    /// 返回 Option<String>：如果有更新，返回最新的 commit SHA；如果没有更新或出错，返回 None
    pub async fn check_skill_update(
        &self,
        owner: &str,
        repo: &str,
        skill_path: &str,
        installed_commit_sha: Option<&str>,
    ) -> Result<Option<String>> {
        // 如果没有安装的 commit SHA，无法判断是否更新
        let installed_sha = match installed_commit_sha {
            Some(sha) => sha,
            None => {
                log::warn!("技能没有 installed_commit_sha，无法检查更新");
                return Ok(None);
            }
        };

        // 构建 API URL
        let path_param = if skill_path == "." { "" } else { skill_path };
        let url = if path_param.is_empty() {
            format!(
                "{}/repos/{}/{}/commits?per_page=1",
                self.api_base, owner, repo
            )
        } else {
            format!(
                "{}/repos/{}/{}/commits?path={}&per_page=1",
                self.api_base, owner, repo, path_param
            )
        };

        log::info!("检查技能更新: {}", url);

        // 发送请求
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("检查更新时网络请求失败")?;

        let status = response.status();

        if !status.is_success() {
            match status.as_u16() {
                403 => {
                    if let Err(e) = self.check_rate_limit(&response) {
                        return Err(e);
                    }
                    return Err(anyhow::anyhow!("无权限访问该仓库"));
                }
                404 => {
                    log::warn!("技能路径不存在: {}/{}/{}", owner, repo, skill_path);
                    return Ok(None);
                }
                _ => {
                    return Err(anyhow::anyhow!("GitHub API 返回错误: {}", status));
                }
            }
        }

        // 解析响应
        let commits: Vec<GitHubCommit> =
            response.json().await.context("解析 GitHub 提交信息失败")?;

        if let Some(latest_commit) = commits.first() {
            let latest_sha = &latest_commit.sha;

            log::info!(
                "技能 {}/{}/{} - 已安装: {}，最新: {}",
                owner,
                repo,
                skill_path,
                installed_sha,
                latest_sha
            );

            // 比较 SHA（只比较前 7 位，因为可能存储的是短 SHA）
            let installed_short = &installed_sha[..installed_sha.len().min(7)];
            let latest_short = &latest_sha[..7];

            if installed_short != latest_short {
                log::info!("检测到更新可用");
                return Ok(Some(latest_sha.clone()));
            } else {
                log::info!("已是最新版本");
                return Ok(None);
            }
        }

        Ok(None)
    }
}

impl Default for GitHubService {
    fn default() -> Self {
        Self::new()
    }
}
