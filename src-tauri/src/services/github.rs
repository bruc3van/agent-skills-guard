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

/// 下载压缩包的安全上限：100 MiB（压缩后大小）
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;

/// GitHub 默认分支候选列表
pub const DEFAULT_BRANCHES: &[&str] = &["main", "master"];

/// 检查解压目标路径是否安全（无路径遍历）
fn is_safe_path(path: &Path, base: &Path) -> bool {
    // 规范化路径：处理 "."、连续分隔符等，但不解析符号链接
    fn normalize(p: &Path) -> PathBuf {
        let mut result = PathBuf::new();
        for component in p.components() {
            match component {
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::CurDir => {}
                other => result.push(other),
            }
        }
        result
    }
    normalize(path).starts_with(normalize(base))
}

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

#[derive(Debug, Deserialize)]
struct GitHubRepositoryMetadata {
    default_branch: Option<String>,
}

pub struct GitHubService {
    client: Client,
    api_base: String,
}

fn push_unique_branch(branches: &mut Vec<String>, branch: &str) {
    let branch = branch.trim();
    if !branch.is_empty() && !branches.iter().any(|existing| existing == branch) {
        branches.push(branch.to_string());
    }
}

fn archive_branch_candidates(default_branch: Option<&str>) -> Vec<String> {
    let mut branches = Vec::new();

    if let Some(default_branch) = default_branch {
        push_unique_branch(&mut branches, default_branch);
    }

    for branch in DEFAULT_BRANCHES {
        push_unique_branch(&mut branches, branch);
    }

    branches
}

fn format_archive_download_error(errors: &[(String, String)]) -> String {
    if errors.is_empty() {
        return "所有分支均下载失败".to_string();
    }

    if errors.iter().all(|(_, error)| error.contains("404")) {
        return "PRIVATE_REPOSITORY_UNSUPPORTED".to_string();
    }

    let details = errors
        .iter()
        .map(|(branch, error)| format!("{}: {}", branch, error))
        .collect::<Vec<_>>()
        .join("; ");

    format!("尝试的分支均下载失败: {}", details)
}

impl GitHubService {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("agent-skills-guard")
                .timeout(std::time::Duration::from_secs(30)) // 30秒超时
                .connect_timeout(std::time::Duration::from_secs(10)) // 10秒连接超时
                .build()
                .unwrap_or_else(|_| Client::new()),
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
                    let api_calls_remaining = std::sync::atomic::AtomicUsize::new(200);
                    match self
                        .scan_directory(
                            &owner,
                            &repo_name,
                            &item.path,
                            &repo.url,
                            &api_calls_remaining,
                        )
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
        api_calls_remaining: &'a std::sync::atomic::AtomicUsize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Skill>>> + Send + 'a>> {
        Box::pin(async move {
            if api_calls_remaining.load(std::sync::atomic::Ordering::Relaxed) == 0 {
                log::warn!(
                    "scan_directory: API 调用次数已达上限，停止扫描 path={}",
                    path
                );
                return Ok(Vec::new());
            }
            api_calls_remaining.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

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
                        if api_calls_remaining.load(std::sync::atomic::Ordering::Relaxed) == 0 {
                            log::warn!(
                                "scan_directory: API 调用次数已达上限，跳过子目录 {}",
                                item.path
                            );
                            continue;
                        }
                        api_calls_remaining.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        match self
                            .scan_directory(owner, repo, &item.path, repo_url, api_calls_remaining)
                            .await
                        {
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
                                            .unwrap_or_default()
                                            .as_secs()
                                            as i64;
                                        let wait_seconds = reset_timestamp - now;

                                        if wait_seconds > 0 {
                                            let wait_minutes = (wait_seconds + 59) / 60; // 向上取整
                                            anyhow::bail!("GITHUB_RATE_LIMITED: {}", wait_minutes);
                                        }
                                    }
                                }
                            }
                            anyhow::bail!("GITHUB_RATE_LIMITED");
                        }
                    }
                    anyhow::bail!("GITHUB_REPO_FORBIDDEN");
                }
                404 => {
                    anyhow::bail!("GITHUB_REPO_NOT_FOUND: {}/{}", owner, repo);
                }
                401 => {
                    anyhow::bail!("GITHUB_UNAUTHORIZED");
                }
                500..=599 => {
                    anyhow::bail!("GITHUB_SERVER_ERROR");
                }
                _ => {
                    anyhow::bail!("GITHUB_API_ERROR: {}", status);
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
                            anyhow::bail!("GITHUB_RATE_LIMITED");
                        }
                    }
                    anyhow::bail!("GITHUB_REPO_FORBIDDEN");
                }
                404 => {
                    anyhow::bail!("GITHUB_REPO_NOT_FOUND: {}", download_url);
                }
                _ => {
                    anyhow::bail!("NETWORK_ERROR: HTTP {}", status);
                }
            }
        }

        const MAX_FILE_BYTES: usize = 2 * 1024 * 1024; // 2 MiB，SKILL.md 等文件不应超过此限制
        let bytes = response.bytes().await.context("读取文件内容失败")?;
        if bytes.len() > MAX_FILE_BYTES {
            anyhow::bail!(
                "NETWORK_ERROR: 文件大小 {} 字节超过限制 {} 字节",
                bytes.len(),
                MAX_FILE_BYTES
            );
        }

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
        let mut last_error = None;

        for branch in DEFAULT_BRANCHES {
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
        // 兼容 BOM 和前导空白（Windows 克隆常见）
        let content = content.trim_start_matches('\u{feff}').trim_start();

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

        // 2. 尝试下载压缩包：优先使用 GitHub 返回的默认分支，再回退到常见分支名。
        let default_branch = self.fetch_repository_default_branch(owner, repo).await?;
        let branches = archive_branch_candidates(default_branch.as_deref());
        let mut errors = Vec::new();
        let mut response = None;

        for branch in branches {
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
                        errors.push((branch, "404 Not Found".to_string()));
                        continue;
                    } else {
                        errors.push((branch, format!("HTTP {}", resp.status())));
                        continue;
                    }
                }
                Err(e) => {
                    log::warn!("请求分支 {} 时发生错误: {}", branch, e);
                    errors.push((branch, format!("请求失败: {}", e)));
                    continue;
                }
            }
        }

        let response =
            response.ok_or_else(|| anyhow::anyhow!(format_archive_download_error(&errors)))?;

        // 3. 保存压缩包到本地（先检查大小限制）
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_ARCHIVE_BYTES {
                return Err(anyhow::anyhow!(
                    "压缩包大小 ({:.1} MB) 超过安全上限 ({:.1} MB)，可能为恶意仓库",
                    content_length as f64 / (1024.0 * 1024.0),
                    MAX_ARCHIVE_BYTES as f64 / (1024.0 * 1024.0)
                ));
            }
        }

        let archive_path = repo_cache_dir.join("archive.zip");
        let bytes = response.bytes().await.context("读取压缩包内容失败")?;

        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err(anyhow::anyhow!(
                "压缩包实际大小 ({:.1} MB) 超过安全上限 ({:.1} MB)",
                bytes.len() as f64 / (1024.0 * 1024.0),
                MAX_ARCHIVE_BYTES as f64 / (1024.0 * 1024.0)
            ));
        }

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

        // 将 SHA 写入元数据文件，供后续可靠的缓存复用判断
        let sha_file = extract_dir.join(".commit_sha");
        if let Err(e) = fs::write(&sha_file, &commit_sha) {
            log::warn!("无法写入 commit SHA 元数据文件: {}", e);
        }

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
                Some(path) => {
                    let candidate = extract_dir.join(path);
                    if !is_safe_path(&candidate, extract_dir) {
                        log::warn!("ZIP条目尝试路径遍历，跳过: {}", file.name());
                        continue;
                    }
                    candidate
                }
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

    async fn fetch_repository_default_branch(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Option<String>> {
        let url = format!("{}/repos/{}/{}", self.api_base, owner, repo);
        let response = match self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                log::warn!("获取仓库默认分支失败: {}", e);
                return Ok(None);
            }
        };

        if let Err(e) = self.check_rate_limit(&response) {
            return Err(e);
        }

        let status = response.status();
        if !status.is_success() {
            log::warn!(
                "无法获取仓库默认分支: {}/{}, HTTP状态码: {}",
                owner,
                repo,
                status
            );
            return Ok(None);
        }

        let metadata: GitHubRepositoryMetadata = match response.json().await {
            Ok(metadata) => metadata,
            Err(e) => {
                log::warn!("解析仓库默认分支失败: {}", e);
                return Ok(None);
            }
        };

        Ok(metadata
            .default_branch
            .filter(|branch| !branch.trim().is_empty()))
    }

    /// 获取仓库默认分支的最新 commit SHA（使用 1 次 API 调用）
    /// 用于缓存复用前的 SHA 比对，判断远端是否有新提交。
    pub async fn get_repository_default_branch_sha(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Option<String>> {
        let url = format!(
            "{}/repos/{}/{}/commits?per_page=1",
            self.api_base, owner, repo
        );

        let response = match self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => return Ok(None), // 网络错误，不阻塞缓存使用
        };

        if !response.status().is_success() {
            return Ok(None); // API 错误时不阻塞缓存使用
        }

        let commits: Vec<serde_json::Value> = match response.json().await {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        if let Some(first) = commits.first() {
            if let Some(sha) = first.get("sha").and_then(|s| s.as_str()) {
                return Ok(Some(sha.to_string()));
            }
        }

        Ok(None)
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
                                    "GITHUB_RATE_LIMITED: {}",
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
        // 优先读取 .commit_sha 元数据文件（由 download_repository_archive 写入）
        let sha_file = extract_dir.join(".commit_sha");
        if sha_file.exists() {
            if let Ok(content) = fs::read_to_string(&sha_file) {
                let sha = content.trim().to_string();
                if !sha.is_empty() {
                    log::info!("从 .commit_sha 元数据文件读取到 SHA: {}", sha);
                    return Ok(sha);
                }
            }
        }

        // 回退：从解压后的子目录名解析（{owner}-{repo}-{commit_sha}）
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

    /// 查询某仓库路径下最新一次（在可选的 ref_sha 之前或包含 ref_sha）的提交 SHA。
    ///
    /// - `skill_path` 为 "." 或空时不带 path 过滤，等价于查询仓库最新提交。
    /// - `ref_sha` 为 Some 时，结果是「在该提交（含）之前最近一次触及该路径的提交」。
    /// - `ref_sha` 为 None 时，结果是「当前 HEAD 上最近一次触及该路径的提交」。
    ///
    /// 找不到任何提交时返回 Ok(None)；遇到 403/网络错误时返回 Err 让调用方决定。
    pub async fn fetch_latest_commit_sha_for_path(
        &self,
        owner: &str,
        repo: &str,
        skill_path: &str,
        ref_sha: Option<&str>,
    ) -> Result<Option<String>> {
        let url = format!("{}/repos/{}/{}/commits", self.api_base, owner, repo);
        let path_param = if skill_path == "." { "" } else { skill_path };

        // 用 reqwest 的 .query() 自动 percent-encode，避免 path/sha 含空格、`#` 等字符时拼接出错
        let mut query: Vec<(&str, &str)> = vec![("per_page", "1")];
        if !path_param.is_empty() {
            query.push(("path", path_param));
        }
        if let Some(s) = ref_sha {
            if !s.is_empty() {
                query.push(("sha", s));
            }
        }

        log::info!(
            "查询提交 SHA: {} (path={:?}, sha={:?})",
            url,
            path_param,
            ref_sha
        );

        let response = self
            .client
            .get(&url)
            .query(&query)
            .send()
            .await
            .context("查询提交信息时网络请求失败")?;

        let status = response.status();
        if !status.is_success() {
            match status.as_u16() {
                403 => {
                    if let Err(e) = self.check_rate_limit(&response) {
                        return Err(e);
                    }
                    return Err(anyhow::anyhow!("GITHUB_REPO_FORBIDDEN"));
                }
                404 => {
                    log::warn!("仓库或路径不存在: {}/{}/{}", owner, repo, skill_path);
                    return Ok(None);
                }
                _ => {
                    return Err(anyhow::anyhow!("GitHub API 返回错误: {}", status));
                }
            }
        }

        let commits: Vec<GitHubCommit> =
            response.json().await.context("解析 GitHub 提交信息失败")?;

        Ok(commits.into_iter().next().map(|c| c.sha))
    }

    /// 检查技能是否有更新。
    ///
    /// 子目录 skill 兼容策略：installed_commit_sha 历史上可能记录的是「仓库 HEAD」而非
    /// 「最近触及该子目录的提交」，会导致两者永不相等而误报更新。本函数在首轮比对未通过时
    /// 追加一次「以 installed_sha 为参考」的查询，若两次结果一致说明用户实际内容已是最新；
    /// 此时通过 `UpToDate { canonical_sha: Some(_) }` 把规范 SHA 回报给调用方，
    /// 由调用方写回数据库完成「自愈」，避免每次检查都付出 2× API 调用。
    pub async fn check_skill_update(
        &self,
        owner: &str,
        repo: &str,
        skill_path: &str,
        installed_commit_sha: Option<&str>,
    ) -> Result<SkillUpdateStatus> {
        let installed_sha = match installed_commit_sha {
            Some(sha) if !sha.is_empty() => sha,
            _ => {
                log::warn!("技能没有 installed_commit_sha，无法检查更新");
                return Ok(SkillUpdateStatus::Unknown);
            }
        };

        // Step 1: HEAD 上路径过滤后的最新提交
        let path_latest = match self
            .fetch_latest_commit_sha_for_path(owner, repo, skill_path, None)
            .await?
        {
            Some(sha) => sha,
            None => return Ok(SkillUpdateStatus::Unknown),
        };

        log::info!(
            "技能 {}/{}/{} - 已安装: {}，路径最新: {}",
            owner,
            repo,
            skill_path,
            installed_sha,
            path_latest
        );

        if shas_match(installed_sha, &path_latest) {
            log::info!("已是最新版本");
            return Ok(SkillUpdateStatus::UpToDate {
                canonical_sha: None,
            });
        }

        // Step 2: 容错。以 installed_sha 为参考查询「截至该提交时」路径上的最新提交。
        // 若与 HEAD 上路径最新一致，说明 installed_sha 是不触及该路径的较新提交，
        // 用户当前文件内容已等同 path_latest，无需更新。
        match self
            .fetch_latest_commit_sha_for_path(owner, repo, skill_path, Some(installed_sha))
            .await
        {
            Ok(Some(historical_path_latest)) => {
                if shas_match(&historical_path_latest, &path_latest) {
                    log::info!(
                        "已是最新版本 (installed_sha {} 未触及路径，建议回写为 {})",
                        installed_sha,
                        path_latest
                    );
                    // 把规范 SHA 报回去，调用方写回数据库后下次只需一次 API 调用即可判定。
                    return Ok(SkillUpdateStatus::UpToDate {
                        canonical_sha: Some(path_latest),
                    });
                }
            }
            Ok(None) => {
                log::warn!("回退查询未返回提交，按有更新处理");
            }
            Err(e) => {
                log::warn!("回退查询失败 (按有更新处理): {}", e);
            }
        }

        log::info!("检测到更新可用");
        Ok(SkillUpdateStatus::UpdateAvailable {
            latest_sha: path_latest,
        })
    }
}

/// 技能更新检查结果。
#[derive(Debug, Clone)]
pub enum SkillUpdateStatus {
    /// 已是最新版本。
    /// `canonical_sha` 是 Some 时，表示 installed_sha 与规范的 path-aware SHA 等价但形态不同
    /// （例如旧数据里记录的是仓库 HEAD），调用方应把它写回数据库以避免后续重复的回退查询。
    UpToDate { canonical_sha: Option<String> },
    /// 有更新可用。
    UpdateAvailable { latest_sha: String },
    /// 无法判断（installed_sha 缺失、仓库/路径不存在等）。
    Unknown,
}

/// 短 SHA 前缀匹配的最小长度。Git 默认短 SHA 是 7 字符，低于此阈值前缀匹配
/// 容易出现碰撞或误吞错误（例如 `installed_sha = "ab"` 会匹配任何以 ab 开头的提交）。
const MIN_SHA_PREFIX_LEN: usize = 7;

/// SHA 等价比较：兼容短 SHA（前缀），但短的一侧必须至少 [`MIN_SHA_PREFIX_LEN`] 字符。
///
/// - 任一侧为空：false
/// - 等长：要求完全相等
/// - 不等长：取短侧作前缀；短侧短于 7 字符时拒绝匹配，避免脏数据被静默接受
fn shas_match(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if short.len() < long.len() && short.len() < MIN_SHA_PREFIX_LEN {
        return false;
    }
    long.starts_with(short)
}

impl Default for GitHubService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_branch_candidates_puts_default_branch_first_without_duplicates() {
        assert_eq!(
            archive_branch_candidates(Some("main")),
            vec!["main".to_string(), "master".to_string()]
        );

        assert_eq!(
            archive_branch_candidates(Some("release")),
            vec![
                "release".to_string(),
                "main".to_string(),
                "master".to_string()
            ]
        );
    }

    #[test]
    fn archive_download_error_includes_every_attempted_branch_for_non_404_errors() {
        let error = format_archive_download_error(&[
            (
                "main".to_string(),
                "HTTP 500 Internal Server Error".to_string(),
            ),
            (
                "master".to_string(),
                "HTTP 500 Internal Server Error".to_string(),
            ),
        ]);

        assert!(error.contains("尝试的分支均下载失败"));
        assert!(error.contains("main: HTTP 500 Internal Server Error"));
        assert!(error.contains("master: HTTP 500 Internal Server Error"));
    }

    #[test]
    fn archive_download_error_reports_private_repo_unsupported_for_404s() {
        let error = format_archive_download_error(&[
            ("main".to_string(), "404 Not Found".to_string()),
            ("master".to_string(), "404 Not Found".to_string()),
        ]);

        assert_eq!(error, "PRIVATE_REPOSITORY_UNSUPPORTED");
        assert!(!error.contains("main"));
        assert!(!error.contains("master"));
    }

    #[test]
    fn shas_match_rejects_empty_inputs() {
        assert!(!shas_match("", "abcdef1234"));
        assert!(!shas_match("abcdef1234", ""));
        assert!(!shas_match("", ""));
    }

    #[test]
    fn shas_match_accepts_exact_equal_shas() {
        let sha = "abcdef1234567890abcdef1234567890abcdef12";
        assert!(shas_match(sha, sha));
    }

    #[test]
    fn shas_match_rejects_equal_length_non_equal() {
        let a = "abcdef1234567890abcdef1234567890abcdef12";
        let b = "abcdef1234567890abcdef1234567890abcdef34";
        assert!(!shas_match(a, b));
    }

    #[test]
    fn shas_match_accepts_valid_short_sha_prefix() {
        // 7 字符短 SHA 是 git 默认长度
        let installed = "abcdef1";
        let latest = "abcdef1234567890abcdef1234567890abcdef12";
        assert!(shas_match(installed, latest));
        assert!(shas_match(latest, installed)); // 顺序无关
    }

    #[test]
    fn shas_match_rejects_too_short_prefix_even_if_matching() {
        // 防止脏数据被静默吞掉：低于 7 字符的前缀不应触发匹配
        let installed = "ab";
        let latest = "abcdef1234567890abcdef1234567890abcdef12";
        assert!(!shas_match(installed, latest));
        assert!(!shas_match(latest, installed));
    }

    #[test]
    fn shas_match_rejects_short_prefix_that_does_not_match() {
        let installed = "deadbee";
        let latest = "abcdef1234567890abcdef1234567890abcdef12";
        assert!(!shas_match(installed, latest));
    }
}
