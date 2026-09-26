use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Read;
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

    #[cfg(test)]
    pub(crate) fn env_var_for_test(&self, key: &str) -> Option<&str> {
        self.env_vars
            .iter()
            .find(|(env_key, _)| env_key == key)
            .map(|(_, val)| val.as_str())
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
        #[cfg(windows)]
        {
            let resolved = resolve_command_path(&self.command);
            if is_cmd_script(&resolved) {
                super::windows_command::validate_cmd_argument(&resolved.to_string_lossy())?;
                for arg in &command.args {
                    super::windows_command::validate_cmd_argument(arg)?;
                }
            }
        }
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
        let writer = pair.master.take_writer().context("无法写入 PTY")?;

        let (tx, rx) = mpsc::sync_channel::<String>(64);
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
            read_until_exit_with_prompts(&rx, child.as_mut(), command.timeout)?;

        drop(writer);
        drop(rx); // unblock a bounded sender before joining the reader
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

const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
fn append_output(buffer: &mut String, chunk: &str) -> bool {
    if buffer.len().saturating_add(chunk.len()) > MAX_OUTPUT_BYTES {
        buffer.push_str("\n[CLI_OUTPUT_LIMIT: command stopped]\n");
        return false;
    }
    buffer.push_str(chunk);
    true
}

fn read_until_exit_with_prompts(
    rx: &mpsc::Receiver<String>,
    child: &mut dyn portable_pty::Child,
    timeout: Duration,
) -> Result<(String, bool)> {
    let start = Instant::now();
    let mut buffer = String::new();
    let mut exit_success = false;
    let mut prompt_window = String::new();

    loop {
        if start.elapsed() >= timeout {
            break;
        }

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                if !append_output(&mut buffer, &chunk) {
                    return Ok((buffer, false));
                }
                prompt_window.push_str(&chunk);
                if prompt_window.len() > 8192 {
                    let mut start = prompt_window.len() - 8192;
                    while !prompt_window.is_char_boundary(start) {
                        start += 1;
                    }
                    prompt_window.drain(..start);
                }
                if is_unsupported_interactive_prompt(&prompt_window) {
                    buffer.push_str(
                        "\nUnsupported interactive prompt detected. If Claude asks you to trust the workspace, open Claude in this folder, review and confirm the trust prompt manually, then retry this operation. Other interactive prompts must also be completed in a terminal.\n",
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
                // EOF can precede process reaping. Keep polling within the command
                // timeout instead of treating an unavailable exit status as failure.
                match child.try_wait() {
                    Ok(Some(status)) => {
                        exit_success = status.success();
                        break;
                    }
                    Ok(None) => {
                        thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(_) => break,
                }
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
            Ok(chunk) => {
                if !append_output(&mut buffer, &chunk) {
                    exit_success = false;
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok((buffer, exit_success))
}

fn is_workspace_trust_prompt_lowercase(text: &str) -> bool {
    text.contains("quick safety check")
        || (text.contains("trust this folder") && text.contains("enter to confirm"))
        || (text.contains("accessing workspace") && text.contains("trust"))
}

fn is_unsupported_interactive_prompt(output: &str) -> bool {
    let text = output.to_lowercase();
    is_workspace_trust_prompt_lowercase(&text)
        || text.contains("password:")
        || text.contains("[sudo] password")
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
        assert!(!is_unsupported_interactive_prompt(
            "Run as administrator to continue"
        ));
        assert!(!is_unsupported_interactive_prompt(
            "warning: permission denied for optional file; continuing"
        ));
        assert!(is_unsupported_interactive_prompt(
            "Quick safety check: Trust this folder? Enter to confirm"
        ));
        assert!(!is_unsupported_interactive_prompt("updated 1 package"));
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;
    #[test]
    fn logs_are_bounded_and_ordinary_errors_are_not_prompts() {
        assert!(!is_unsupported_interactive_prompt(
            "permission denied: optional cache; retrying"
        ));
        let mut text = "x".repeat(MAX_OUTPUT_BYTES);
        assert!(!append_output(&mut text, "excess"));
        assert!(text.len() < MAX_OUTPUT_BYTES + 100);
    }
}

#[cfg(test)]
mod exit_race_regression {
    use super::*;
    use portable_pty::{Child, ChildKiller, ExitStatus};
    #[derive(Debug, Clone)]
    struct DelayedExit {
        polls: usize,
        ready_after: usize,
    }
    impl ChildKiller for DelayedExit {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(self.clone())
        }
    }
    impl Child for DelayedExit {
        fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
            self.polls += 1;
            Ok((self.polls >= self.ready_after).then(|| ExitStatus::with_exit_code(0)))
        }
        fn wait(&mut self) -> std::io::Result<ExitStatus> {
            panic!("must not block")
        }
        fn process_id(&self) -> Option<u32> {
            None
        }
        #[cfg(windows)]
        fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
            None
        }
    }
    #[test]
    fn output_eof_waits_for_exit_status_but_respects_timeout() {
        let (tx, rx) = mpsc::sync_channel(1);
        drop(tx);
        let mut child = DelayedExit {
            polls: 0,
            ready_after: 3,
        };
        assert!(
            read_until_exit_with_prompts(&rx, &mut child, Duration::from_secs(1))
                .unwrap()
                .1
        );
        let mut running = DelayedExit {
            polls: 0,
            ready_after: usize::MAX,
        };
        assert!(
            !read_until_exit_with_prompts(&rx, &mut running, Duration::from_millis(30))
                .unwrap()
                .1
        );
    }
}
