/// 跨平台目录链接工具
/// Windows 使用 NTFS Junction（无需管理员权限）
/// macOS/Linux 使用 POSIX symlink
use anyhow::{Context, Result};
use std::path::Path;

fn paths_point_to_same_location(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// 创建目录链接：link -> source
/// 若 link 已存在（链接或目录），先删除再创建
pub fn create_dir_link(source: &Path, link: &Path) -> Result<()> {
    // 确保源目录存在
    if !source.exists() {
        std::fs::create_dir_all(source).context(format!("无法创建源目录: {:?}", source))?;
    }

    // 确保父目录存在
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).context(format!("无法创建链接父目录: {:?}", parent))?;
    }

    // 若目标已存在，先删除
    if link.exists() || is_dir_link(link) {
        if paths_point_to_same_location(source, link) {
            return Ok(());
        }

        let metadata = std::fs::symlink_metadata(link)
            .context(format!("无法读取已有链接/目录: {:?}", link))?;
        if is_dir_link(link) {
            remove_dir_link(link).context(format!("无法删除已有链接: {:?}", link))?;
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(link).context(format!("无法删除已有目录: {:?}", link))?;
        } else {
            std::fs::remove_file(link).context(format!("无法删除已有文件: {:?}", link))?;
        }
    }

    create_link_impl(source, link)
}

/// 检查路径是否为目录链接（Junction 或 symlink）
pub fn is_dir_link(path: &Path) -> bool {
    #[cfg(windows)]
    {
        is_junction(path)
            || path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }
}

/// 读取链接目标路径
pub fn read_dir_link_target(link: &Path) -> Result<std::path::PathBuf> {
    #[cfg(windows)]
    {
        read_junction_or_symlink_target(link)
    }
    #[cfg(not(windows))]
    {
        std::fs::read_link(link).context(format!("无法读取链接目标: {:?}", link))
    }
}

/// 删除目录链接（不删除源）
pub fn remove_dir_link(link: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        // Junction 和普通目录都用 remove_dir；如果是 symlink 也用 remove_dir
        if link.exists() || is_dir_link(link) {
            std::fs::remove_dir(link).context(format!("无法删除目录链接: {:?}", link))?;
        }
    }
    #[cfg(not(windows))]
    {
        if link.symlink_metadata().is_ok() {
            std::fs::remove_file(link).context(format!("无法删除符号链接: {:?}", link))?;
        }
    }
    Ok(())
}

// ── 平台实现 ──────────────────────────────────────────────────

#[cfg(windows)]
fn create_link_impl(source: &Path, link: &Path) -> Result<()> {
    use std::process::Command;

    let source_str = source.to_str().context("源路径包含无效字符")?;
    let link_str = link.to_str().context("链接路径包含无效字符")?;

    let mut command = Command::new("cmd");
    command.args(["/C", "mklink", "/J", link_str, source_str]);
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let output = command.output().context("无法执行 mklink 命令")?;

    // cmd.exe /C 即使内部命令失败也可能返回 0，需要验证 junction 实际创建成功
    if output.status.success() && link.exists() {
        log::info!("创建 Junction: {:?} -> {:?}", link, source);
        return Ok(());
    }

    // Junction 失败时降级为目录拷贝（ReFS、跨卷等场景）
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    log::warn!(
        "mklink /J 失败（exit={}, stderr={}, stdout={}），降级为目录拷贝: {:?} -> {:?}",
        output.status.code().unwrap_or(-1),
        stderr.trim(),
        stdout.trim(),
        source,
        link
    );
    copy_dir_fallback(source, link)
}

#[cfg(not(windows))]
fn create_link_impl(source: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source, link)
        .context(format!("无法创建符号链接: {:?} -> {:?}", link, source))?;
    log::info!("创建 symlink: {:?} -> {:?}", link, source);
    Ok(())
}

#[cfg(windows)]
fn is_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    // Junction 的 file_attributes 包含 FILE_ATTRIBUTE_REPARSE_POINT (0x400)
    // 且 reparse tag 为 IO_REPARSE_TAG_MOUNT_POINT (0xA0000003)
    // 简化判断：是目录且有 reparse point 属性
    if let Ok(meta) = path.symlink_metadata() {
        let attrs = meta.file_attributes();
        const REPARSE: u32 = 0x0400;
        const DIRECTORY: u32 = 0x0010;
        (attrs & REPARSE != 0) && (attrs & DIRECTORY != 0)
    } else {
        false
    }
}

#[cfg(windows)]
fn read_junction_or_symlink_target(link: &Path) -> Result<std::path::PathBuf> {
    // std::fs::read_link 在 Windows 上对 junction 也适用（Rust 1.58+）
    std::fs::read_link(link).context(format!("无法读取链接目标: {:?}", link))
}

/// Junction 创建失败时的降级方案：直接复制目录内容
fn copy_dir_fallback(source: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        // 跳过 Junction / symlink，防止循环递归
        if is_dir_link(&src_path) {
            log::warn!("copy_dir_fallback: 跳过链接 {:?}", src_path);
            continue;
        }
        if ft.is_dir() {
            copy_dir_fallback(&src_path, &dst_path)?;
        } else if ft.is_file() {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_remove_link() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        let link = tmp.path().join("link");

        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "test").unwrap();

        create_dir_link(&source, &link).unwrap();
        assert!(link.exists(), "link 应该存在");
        assert!(link.join("SKILL.md").exists(), "link 内文件应该可访问");

        remove_dir_link(&link).unwrap();
        assert!(!link.exists(), "link 删除后应该不存在");
        assert!(source.exists(), "源目录应该保留");
    }

    #[test]
    fn test_overwrite_existing_link() {
        let tmp = tempdir().unwrap();
        let source1 = tmp.path().join("source1");
        let source2 = tmp.path().join("source2");
        let link = tmp.path().join("link");

        std::fs::create_dir_all(&source1).unwrap();
        std::fs::create_dir_all(&source2).unwrap();
        std::fs::write(source2.join("v2.txt"), "v2").unwrap();

        create_dir_link(&source1, &link).unwrap();
        create_dir_link(&source2, &link).unwrap(); // 覆盖

        assert!(link.join("v2.txt").exists(), "应该指向 source2");
    }

    #[test]
    fn test_overwrite_existing_non_empty_directory() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("source");
        let link = tmp.path().join("link");

        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "source").unwrap();
        std::fs::create_dir_all(&link).unwrap();
        std::fs::write(link.join("old.txt"), "old").unwrap();

        create_dir_link(&source, &link).unwrap();

        assert!(link.join("SKILL.md").exists(), "应该替换为新的源目录链接");
        assert!(!link.join("old.txt").exists(), "旧目录内容应该被替换");
    }

    #[test]
    fn test_is_dir_link() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("src");
        let link = tmp.path().join("lnk");
        std::fs::create_dir_all(&source).unwrap();

        assert!(!is_dir_link(&source));
        create_dir_link(&source, &link).unwrap();
        assert!(is_dir_link(&link));
    }
}
