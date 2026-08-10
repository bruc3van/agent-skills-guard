//! 持久化诊断日志。
//!
//! 发布版此前只有控制台 `env_logger`，进程崩溃后不留任何现场，
//! 导致「打开即闪退」这类问题无法归因。本模块提供：
//!
//! - 同时写入 stderr 与滚动日志文件的 tee writer
//! - `panic` 钩子，把 panic 消息、位置与 backtrace 落盘
//!
//! 日志目录与应用数据目录同级：`<data_dir>/<identifier>/logs/`。
//! 之所以不用 Tauri 的 `app_data_dir()`，是因为日志必须在 Tauri 运行时
//! 启动**之前**就绪——启动期的 panic 恰恰是最需要记录的。

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 应用标识符，与 `tauri.conf.json` 的 `identifier` 保持一致
const APP_IDENTIFIER: &str = "com.agent-skills-guard.app";
const LOG_FILE_NAME: &str = "agent-skills-guard.log";
/// 单个日志文件上限，超过后轮转为 `.1`
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

static LOG_FILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 解析日志目录：`<data_dir>/<identifier>/logs`
fn resolve_log_dir() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join(APP_IDENTIFIER).join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// 返回当前日志文件路径（若日志初始化失败则为 `None`）
pub fn log_file_path() -> Option<&'static Path> {
    LOG_FILE_PATH.get()?.as_deref()
}

/// 超过上限时把当前日志轮转为 `.1`，只保留一代历史，避免无限增长
fn rotate_if_needed(path: &Path) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() < MAX_LOG_BYTES {
        return;
    }
    let rotated = path.with_extension("log.1");
    let _ = std::fs::remove_file(&rotated);
    let _ = std::fs::rename(path, &rotated);
}

fn open_log_file(path: &Path) -> io::Result<File> {
    rotate_if_needed(path);
    OpenOptions::new().create(true).append(true).open(path)
}

/// 把日志同时写到 stderr 和文件。任一目标写失败都不影响另一个，
/// 也绝不向上传播错误——日志系统本身不应成为故障源。
struct TeeWriter {
    file: Option<Mutex<File>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.flush();
            }
        }
        Ok(())
    }
}

/// 初始化日志。必须在 Tauri / Tokio 运行时启动前从 `main()` 调用一次。
pub fn init() {
    let file_path = resolve_log_dir().map(|dir| dir.join(LOG_FILE_NAME));
    let file = file_path
        .as_deref()
        .and_then(|path| match open_log_file(path) {
            Ok(file) => Some(Mutex::new(file)),
            Err(e) => {
                eprintln!("无法打开日志文件 {}: {}", path.display(), e);
                None
            }
        });

    let has_file = file.is_some();
    let _ = LOG_FILE_PATH.set(if has_file { file_path.clone() } else { None });

    let writer = TeeWriter { file };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .target(env_logger::Target::Pipe(Box::new(writer)))
        .format(|buf, record| {
            writeln!(
                buf,
                "[{}] [{}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();

    match (has_file, file_path.as_deref()) {
        (true, Some(path)) => log::info!("日志文件: {}", path.display()),
        _ => log::warn!("未能创建日志文件，日志仅输出到控制台"),
    }

    install_panic_hook();
}

/// 安装 panic 钩子，把崩溃现场写入日志。
///
/// 保留原有钩子的行为（继续打印到 stderr），只是额外落盘。
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());

        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());

        let backtrace = std::backtrace::Backtrace::force_capture();

        log::error!(
            "PANIC at {} in thread '{}': {}\nbacktrace:\n{}",
            location,
            std::thread::current().name().unwrap_or("<unnamed>"),
            message,
            backtrace
        );
        log::logger().flush();

        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn rotate_moves_oversized_log_and_starts_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");

        std::fs::write(&path, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();
        let mut file = open_log_file(&path).unwrap();
        file.write_all(b"fresh").unwrap();
        drop(file);

        let rotated = path.with_extension("log.1");
        assert!(rotated.exists(), "oversized log should be rotated to .1");

        let mut current = String::new();
        File::open(&path)
            .unwrap()
            .read_to_string(&mut current)
            .unwrap();
        assert_eq!(current, "fresh", "current log should restart empty");
    }

    #[test]
    fn rotate_keeps_small_log_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");

        std::fs::write(&path, b"existing\n").unwrap();
        let mut file = open_log_file(&path).unwrap();
        file.write_all(b"appended\n").unwrap();
        drop(file);

        assert!(!path.with_extension("log.1").exists());

        let mut content = String::new();
        File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "existing\nappended\n", "small log should append");
    }

    #[test]
    fn tee_writer_survives_missing_file_target() {
        let mut writer = TeeWriter { file: None };
        assert_eq!(writer.write(b"hello").unwrap(), 5);
        assert!(writer.flush().is_ok());
    }
}
