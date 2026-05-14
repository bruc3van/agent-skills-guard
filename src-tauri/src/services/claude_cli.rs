use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
#[cfg(windows)]
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(windows)]
use which::which;

pub struct ClaudeCommand {
    pub args: Vec<String>,
    pub timeout: Duration,
}

pub struct ClaudeCommandOutput {
    pub command: String,
    pub output: String,
    pub exit_success: bool,
}

pub struct ClaudeCliResult {
    pub outputs: Vec<ClaudeCommandOutput>,
    pub raw_log: String,
    pub exit_success: bool,
}

pub struct ClaudeCli {
    command: String,
    env_remove_prefixes: Vec<String>,
    env_vars: Vec<(String, String)>,
}

impl ClaudeCli {
    pub fn new(command: String) -> Self {
        Self {
            command,
            env_remove_prefixes: Vec::new(),
            env_vars: Vec::new(),
        }
    }

    pub fn env_remove_prefix(mut self, prefix: &str) -> Self {
        self.env_remove_prefixes.push(prefix.to_string());
        self
    }

    pub fn env_var(mut self, key: &str, val: &str) -> Self {
        self.env_vars.push((key.to_string(), val.to_string()));
        self
    }

    pub fn run(&self, commands: &[ClaudeCommand]) -> Result<ClaudeCliResult> {
        let mut raw_log = String::new();
        let mut outputs = Vec::new();
        let mut all_success = true;
        for command in commands {
            let (output, exit_success) = self.run_command(command)?;
            raw_log.push_str(&output);
            if !exit_success {
                all_success = false;
            }
            outputs.push(ClaudeCommandOutput {
                command: command.args.join(" "),
                output,
                exit_success,
            });
        }

        Ok(ClaudeCliResult {
            outputs,
            raw_log,
            exit_success: all_success,
        })
    }

    fn run_command(&self, command: &ClaudeCommand) -> Result<(String, bool)> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("无法创建 PTY")?;

        let cmd = self.build_command_builder(command);
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .context("无法启动 Claude CLI")?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("无法读取 PTY 输出")?;
        let mut writer = pair.master.take_writer().context("无法写入 PTY")?;

        let (tx, rx) = mpsc::channel::<String>();
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                        if tx.send(chunk).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let (output, exit_success) =
            read_until_exit_with_prompts(&rx, &mut writer, child.as_mut(), command.timeout)?;

        drop(writer);
        let _ = child.kill();

        let wait_start = Instant::now();
        while wait_start.elapsed() < Duration::from_secs(2) {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }

        let join_start = Instant::now();
        while !reader_handle.is_finished() && join_start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(50));
        }

        if reader_handle.is_finished() {
            let _ = reader_handle.join();
        } else {
            log::warn!("PTY reader thread did not finish in time; detaching.");
        }

        Ok((output, exit_success))
    }

    fn build_command_builder(&self, command: &ClaudeCommand) -> CommandBuilder {
        let mut cmd = self.build_base_command_builder(command);
        apply_env_remove_prefixes(&mut cmd, &self.env_remove_prefixes);
        for (key, val) in &self.env_vars {
            cmd.env(key, val);
        }
        cmd
    }

    fn build_base_command_builder(&self, command: &ClaudeCommand) -> CommandBuilder {
        #[cfg(windows)]
        {
            let resolved = resolve_command_path(&self.command);
            if is_cmd_script(&resolved) {
                let comspec = std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string());
                let mut cmd = CommandBuilder::new(comspec);
                cmd.arg("/d");
                cmd.arg("/c");
                cmd.arg(&resolved);
                cmd.args(command.args.iter());
                return cmd;
            }

            let mut cmd = CommandBuilder::new(resolved);
            cmd.args(command.args.iter());
            return cmd;
        }

        #[cfg(not(windows))]
        {
            let mut cmd = CommandBuilder::new(&self.command);
            cmd.args(command.args.iter());
            cmd
        }
    }
}

#[cfg(windows)]
fn resolve_command_path(command: &str) -> PathBuf {
    which(command).unwrap_or_else(|_| PathBuf::from(command))
}

#[cfg(windows)]
fn is_cmd_script(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "cmd" | "bat"))
        .unwrap_or(false)
}

fn apply_env_remove_prefixes(cmd: &mut CommandBuilder, prefixes: &[String]) {
    if prefixes.is_empty() {
        return;
    }
    for (key, _) in std::env::vars() {
        if prefixes
            .iter()
            .any(|prefix| key.starts_with(prefix.as_str()))
        {
            cmd.env_remove(&key);
        }
    }
}

fn read_until_exit_with_prompts(
    rx: &mpsc::Receiver<String>,
    writer: &mut dyn Write,
    child: &mut dyn portable_pty::Child,
    timeout: Duration,
) -> Result<(String, bool)> {
    let start = Instant::now();
    let mut buffer = String::new();
    let mut trust_attempts: u8 = 0;
    let mut last_trust_sent: Option<Instant> = None;
    let mut exit_success = false;

    loop {
        if start.elapsed() >= timeout {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                buffer.push_str(&chunk);

                if is_workspace_trust_prompt(&buffer) && trust_attempts < 3 {
                    let should_send = match last_trust_sent {
                        Some(last) => last.elapsed() >= Duration::from_millis(400),
                        None => true,
                    };
                    if should_send {
                        log::info!("Workspace trust prompt detected; auto-confirming.");
                        send_enter(writer).context("发送信任确认失败")?;
                        trust_attempts += 1;
                        last_trust_sent = Some(Instant::now());
                    }
                }

                if is_unsupported_interactive_prompt(&buffer) {
                    buffer.push_str(
                        "\nUnsupported interactive prompt detected. Please run this command manually in a terminal.\n",
                    );
                    return Ok((buffer, false));
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = child.try_wait() {
                    exit_success = status.success();
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if let Ok(Some(status)) = child.try_wait() {
                    exit_success = status.success();
                }
                break;
            }
        }

        if let Ok(Some(status)) = child.try_wait() {
            exit_success = status.success();
            break;
        }
    }

    let drain_start = Instant::now();
    while drain_start.elapsed() < Duration::from_millis(300) {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => buffer.push_str(&chunk),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok((buffer, exit_success))
}

fn is_workspace_trust_prompt(output: &str) -> bool {
    let text = output.to_lowercase();
    text.contains("quick safety check")
        || (text.contains("trust this folder") && text.contains("enter to confirm"))
        || (text.contains("accessing workspace") && text.contains("trust"))
}

fn is_unsupported_interactive_prompt(output: &str) -> bool {
    let text = output.to_lowercase();
    text.contains("password:")
        || text.contains("[sudo] password")
        || text.contains("administrator permission")
        || text.contains("administrator privileges")
        || text.contains("run as administrator")
        || text.contains("permission denied")
}

fn send_enter(writer: &mut dyn Write) -> Result<()> {
    writer.write_all(line_ending())?;
    writer.flush().ok();
    Ok(())
}

fn line_ending() -> &'static [u8] {
    if cfg!(windows) {
        b"\r\n"
    } else {
        b"\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn wraps_cmd_script_with_cmd_exe() {
        let cli = ClaudeCli::new(r"C:\Users\Bruce\AppData\Roaming\npm\claude.cmd".to_string());
        let command = ClaudeCommand {
            args: vec![
                "plugin".to_string(),
                "list".to_string(),
                "--json".to_string(),
            ],
            timeout: Duration::from_secs(1),
        };

        let builder = cli.build_command_builder(&command);
        let argv = builder
            .get_argv()
            .iter()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(argv[0].to_ascii_lowercase().ends_with("cmd.exe"));
        assert_eq!(argv[1], "/d");
        assert_eq!(argv[2], "/c");
        assert!(argv[3].to_ascii_lowercase().ends_with("claude.cmd"));
        assert_eq!(argv[4], "plugin");
        assert_eq!(argv[5], "list");
        assert_eq!(argv[6], "--json");
    }

    #[test]
    fn detects_unsupported_interactive_prompts() {
        assert!(is_unsupported_interactive_prompt(
            "[sudo] password for user:"
        ));
        assert!(is_unsupported_interactive_prompt("Password:"));
        assert!(is_unsupported_interactive_prompt(
            "Run as administrator to continue"
        ));
        assert!(!is_unsupported_interactive_prompt("updated 1 package"));
    }
}
