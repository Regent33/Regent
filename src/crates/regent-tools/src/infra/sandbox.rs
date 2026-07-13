//! Sandbox policy + the ephemeral-container backend.
//!
//! `REGENT_SANDBOX` turns on enforced isolation: the filesystem jail
//! ([`crate::ToolContext::new_sandboxed`]) for in-process file tools, and a ban
//! on the host `local` terminal backend ([`enforce_backend`]). [`SandboxBackend`]
//! runs each command in a fresh, locked-down `docker run` — no network,
//! read-only rootfs, dropped capabilities, memory/pid limits, with only the
//! workspace (`/work`) and a tmpfs `/tmp` writable — stronger than `docker
//! exec` into a standing container. Argv construction is pure and unit-tested.

use crate::domain::contracts::{CommandOutput, TerminalBackend};
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::path::Path;
use std::time::Duration;

const DEFAULT_MEMORY: &str = "512m";
const DEFAULT_PIDS: u32 = 256;

pub struct SandboxBackend {
    image: String,
    memory: String,
    pids: u32,
}

impl SandboxBackend {
    #[must_use]
    pub fn new(image: impl Into<String>) -> Self {
        Self {
            image: image.into(),
            memory: DEFAULT_MEMORY.to_owned(),
            pids: DEFAULT_PIDS,
        }
    }
}

#[async_trait]
impl TerminalBackend for SandboxBackend {
    fn describe(&self) -> String {
        format!("sandbox:{}", self.image)
    }

    async fn run(
        &self,
        command: &str,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<CommandOutput, RegentError> {
        let argv = build_sandbox_args(&self.image, cwd, &self.memory, self.pids, command);
        super::backends::run_argv(&argv, None, timeout).await
    }
}

/// Builds the locked-down `docker run` argv for an ephemeral sandbox. The host
/// `cwd` is mounted read-write at `/work` (the only writable area besides the
/// tmpfs `/tmp`); the root filesystem is read-only, networking is off, all
/// capabilities are dropped, and memory/pids are capped.
#[must_use]
pub fn build_sandbox_args(
    image: &str,
    cwd: &Path,
    memory: &str,
    pids: u32,
    command: &str,
) -> Vec<String> {
    vec![
        "docker".to_owned(),
        "run".to_owned(),
        "--rm".to_owned(),
        "--network".to_owned(),
        "none".to_owned(),
        "--read-only".to_owned(),
        "--cap-drop".to_owned(),
        "ALL".to_owned(),
        "--security-opt".to_owned(),
        "no-new-privileges".to_owned(),
        "--memory".to_owned(),
        memory.to_owned(),
        "--pids-limit".to_owned(),
        pids.to_string(),
        "--tmpfs".to_owned(),
        "/tmp:rw,exec,nosuid".to_owned(),
        "-v".to_owned(),
        format!("{}:/work:rw", cwd.display()),
        "-w".to_owned(),
        "/work".to_owned(),
        image.to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        command.to_owned(),
    ]
}

/// Whether `REGENT_SANDBOX` requests sandboxed tool execution (filesystem jail
/// + host-backend ban). Truthy values: `1`/`true`/`yes`/`on`.
#[must_use]
pub fn sandbox_enabled() -> bool {
    std::env::var("REGENT_SANDBOX")
        .map(|v| is_truthy(&v))
        .unwrap_or(false)
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Whether an environment variable name looks like a credential — stripped from
/// the environment of every spawned tool command so the agent (or a prompt
/// injection) can't exfiltrate secrets through the shell. Mirrors the reference
/// "API keys stripped from the child env" hardening. Matches API keys, tokens,
/// passwords, session keys, and private keys by name.
#[must_use]
pub fn is_secret_env_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    const NEEDLES: &[&str] = &[
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASSWD",
        "CREDENTIAL",
        "APIKEY",
        "API_KEY",
        "PRIVATE_KEY",
        "SESSION_KEY",
        "ACCESS_KEY",
    ];
    NEEDLES.iter().any(|needle| upper.contains(needle)) || upper.ends_with("_KEY")
}

/// When sandbox mode is on, the host `local` backend is forbidden — execution
/// must be isolated (docker/ssh/sandbox).
pub(crate) fn enforce_backend(backend_describe: &str) -> Result<(), RegentError> {
    enforce(backend_describe, sandbox_enabled())
}

fn enforce(backend_describe: &str, sandbox_on: bool) -> Result<(), RegentError> {
    if sandbox_on && backend_describe == "local" {
        return Err(RegentError::Config(
            "REGENT_SANDBOX is set but REGENT_TERMINAL_BACKEND is 'local'; sandboxed execution \
             requires docker:<container>, sandbox:<image>, or ssh:<dest>"
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
