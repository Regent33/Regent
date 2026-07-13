//! Terminal backends: local shell, docker exec, ssh. Remote backends shell
//! out to the `docker`/`ssh` CLIs (no SDK dependencies); argv construction
//! is pure and unit-tested. Timeouts kill the spawned (client) process.

use crate::domain::contracts::{CommandOutput, TerminalBackend};
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub struct LocalBackend;

#[async_trait]
impl TerminalBackend for LocalBackend {
    fn describe(&self) -> String {
        "local".to_owned()
    }

    async fn run(
        &self,
        command: &str,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<CommandOutput, RegentError> {
        #[cfg(windows)]
        {
            // cmd.exe does NOT understand Rust's default `\"` argument escaping,
            // so a quoted command (e.g. `start "" "https://…"`) reaches it as
            // `start \"\" \"https://…\"`, the `\"` collapse to literal `\`, and
            // it tries to open `\\`. Pass the whole `/C <command>` line verbatim
            // via raw_arg so cmd applies its own quoting rules.
            use std::os::windows::process::CommandExt;
            let mut std_cmd = std::process::Command::new("cmd");
            std_cmd.raw_arg(format!("/C {command}"));
            run_command(tokio::process::Command::from(std_cmd), Some(cwd), timeout).await
        }
        #[cfg(not(windows))]
        {
            let argv = vec!["sh".to_owned(), "-c".to_owned(), command.to_owned()];
            run_argv(&argv, Some(cwd), timeout).await
        }
    }
}

pub struct DockerBackend {
    pub container: String,
    /// Working directory inside the container (host cwd is meaningless there).
    pub workdir: Option<String>,
}

#[async_trait]
impl TerminalBackend for DockerBackend {
    fn describe(&self) -> String {
        format!("docker:{}", self.container)
    }

    async fn run(
        &self,
        command: &str,
        _cwd: &Path,
        timeout: Duration,
    ) -> Result<CommandOutput, RegentError> {
        let argv = build_docker_args(&self.container, self.workdir.as_deref(), command);
        run_argv(&argv, None, timeout).await
    }
}

pub struct SshBackend {
    /// `user@host` (key-based auth; BatchMode forbids password prompts).
    pub destination: String,
}

#[async_trait]
impl TerminalBackend for SshBackend {
    fn describe(&self) -> String {
        format!("ssh:{}", self.destination)
    }

    async fn run(
        &self,
        command: &str,
        _cwd: &Path,
        timeout: Duration,
    ) -> Result<CommandOutput, RegentError> {
        let argv = build_ssh_args(&self.destination, command);
        run_argv(&argv, None, timeout).await
    }
}

#[must_use]
pub fn build_docker_args(container: &str, workdir: Option<&str>, command: &str) -> Vec<String> {
    let mut argv = vec!["docker".to_owned(), "exec".to_owned()];
    if let Some(dir) = workdir {
        argv.push("-w".to_owned());
        argv.push(dir.to_owned());
    }
    argv.extend([
        container.to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        command.to_owned(),
    ]);
    argv
}

#[must_use]
pub fn build_ssh_args(destination: &str, command: &str) -> Vec<String> {
    vec![
        "ssh".to_owned(),
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
        destination.to_owned(),
        command.to_owned(),
    ]
}

/// Parses `REGENT_TERMINAL_BACKEND`: `local` (default), `docker:<container>`
/// (optional `:workdir`), `ssh:<user@host>`.
pub fn terminal_backend_from_env() -> Result<Arc<dyn TerminalBackend>, RegentError> {
    let raw = std::env::var("REGENT_TERMINAL_BACKEND").unwrap_or_else(|_| "local".to_owned());
    let backend = parse_backend(&raw)?;
    crate::infra::sandbox::enforce_backend(&backend.describe())?;
    Ok(backend)
}

pub fn parse_backend(raw: &str) -> Result<Arc<dyn TerminalBackend>, RegentError> {
    match raw.split(':').collect::<Vec<_>>().as_slice() {
        ["local"] | [""] => Ok(Arc::new(LocalBackend)),
        ["docker", container] => Ok(Arc::new(DockerBackend {
            container: (*container).to_owned(),
            workdir: None,
        })),
        ["docker", container, workdir] => Ok(Arc::new(DockerBackend {
            container: (*container).to_owned(),
            workdir: Some((*workdir).to_owned()),
        })),
        ["ssh", destination] => Ok(Arc::new(SshBackend {
            destination: (*destination).to_owned(),
        })),
        ["sandbox", image] => Ok(Arc::new(crate::infra::sandbox::SandboxBackend::new(
            (*image).to_owned(),
        ))),
        _ => Err(RegentError::Config(format!(
            "invalid REGENT_TERMINAL_BACKEND '{raw}' \
             (local | docker:<container>[:workdir] | sandbox:<image> | ssh:<dest>)"
        ))),
    }
}

pub(crate) async fn run_argv(
    argv: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<CommandOutput, RegentError> {
    let mut process = tokio::process::Command::new(&argv[0]);
    process.args(&argv[1..]);
    run_command(process, cwd, timeout).await
}

/// Run a fully-built command: strip credential env, set cwd, enforce the
/// timeout (killing the child), and capture stdout/stderr.
pub(crate) async fn run_command(
    mut process: tokio::process::Command,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<CommandOutput, RegentError> {
    if let Some(dir) = cwd {
        process.current_dir(dir);
    }
    // Strip credentials from the child's environment so a tool command (or a
    // prompt injection driving one) can't exfiltrate secrets through the shell.
    for (key, _) in std::env::vars() {
        if crate::infra::sandbox::is_secret_env_var(&key) {
            process.env_remove(&key);
        }
    }
    process.kill_on_drop(true);
    let result = tokio::time::timeout(timeout, process.output())
        .await
        .map_err(|_| RegentError::Tool {
            tool: "terminal".into(),
            message: format!(
                "command timed out after {}s (process killed)",
                timeout.as_secs()
            ),
        })?
        .map_err(|io| RegentError::Tool {
            tool: "terminal".into(),
            message: format!("spawn: {io}"),
        })?;
    Ok(CommandOutput {
        exit_code: result.status.code(),
        stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    })
}

#[cfg(test)]
#[path = "backends_tests.rs"]
mod tests;
