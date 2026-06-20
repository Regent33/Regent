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
        Self { image: image.into(), memory: DEFAULT_MEMORY.to_owned(), pids: DEFAULT_PIDS }
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
    std::env::var("REGENT_SANDBOX").map(|v| is_truthy(&v)).unwrap_or(false)
}

fn is_truthy(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Whether an environment variable name looks like a credential — stripped from
/// the environment of every spawned tool command so the agent (or a prompt
/// injection) can't exfiltrate secrets through the shell. Mirrors Hermes's
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
mod tests {
    use super::*;

    #[test]
    fn sandbox_argv_is_locked_down() {
        let argv = build_sandbox_args("alpine", Path::new("/srv/work"), "256m", 128, "ls -la");
        assert_eq!(argv[0], "docker");
        assert_eq!(argv[1], "run");
        assert!(argv.contains(&"--rm".to_owned()));
        assert!(argv.windows(2).any(|w| w[0] == "--network" && w[1] == "none"));
        assert!(argv.contains(&"--read-only".to_owned()));
        assert!(argv.windows(2).any(|w| w[0] == "--cap-drop" && w[1] == "ALL"));
        assert!(argv.windows(2).any(|w| w[0] == "--memory" && w[1] == "256m"));
        assert!(argv.windows(2).any(|w| w[0] == "--pids-limit" && w[1] == "128"));
        assert!(argv.windows(2).any(|w| w[0] == "-v" && w[1] == "/srv/work:/work:rw"));
        assert_eq!(argv.last().unwrap(), "ls -la");
        assert_eq!(SandboxBackend::new("alpine").describe(), "sandbox:alpine");
    }

    #[test]
    fn truthy_parsing() {
        assert!(is_truthy("1") && is_truthy("TRUE") && is_truthy("on") && is_truthy(" yes "));
        assert!(!is_truthy("0") && !is_truthy("") && !is_truthy("no"));
    }

    #[test]
    fn secret_env_detection() {
        for key in [
            "REGENT_API_KEY",
            "ANTHROPIC_API_KEY",
            "SLACK_BOT_TOKEN",
            "TWILIO_AUTH_TOKEN",
            "DISCORD_PUBLIC_KEY",
            "DB_PASSWORD",
            "MY_SECRET",
            "AWS_SECRET_ACCESS_KEY",
            "ssh_private_key",
        ] {
            assert!(is_secret_env_var(key), "{key} should be treated as secret");
        }
        for key in ["PATH", "HOME", "LANG", "TERM", "USER", "CARGO_HOME", "REGENT_HOME"] {
            assert!(!is_secret_env_var(key), "{key} should not be treated as secret");
        }
    }

    #[test]
    fn sandbox_mode_forbids_only_the_local_backend() {
        assert!(enforce("local", true).is_err());
        assert!(enforce("local", false).is_ok());
        assert!(enforce("docker:dev", true).is_ok());
        assert!(enforce("sandbox:alpine", true).is_ok());
        assert!(enforce("ssh:me@box", true).is_ok());
    }
}
