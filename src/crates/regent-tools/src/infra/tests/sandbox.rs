//! Unit tests for `sandbox` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

#[test]
fn sandbox_argv_is_locked_down() {
    let argv = build_sandbox_args("alpine", Path::new("/srv/work"), "256m", 128, "ls -la");
    assert_eq!(argv[0], "docker");
    assert_eq!(argv[1], "run");
    assert!(argv.contains(&"--rm".to_owned()));
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--network" && w[1] == "none")
    );
    assert!(argv.contains(&"--read-only".to_owned()));
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--cap-drop" && w[1] == "ALL")
    );
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--memory" && w[1] == "256m")
    );
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "--pids-limit" && w[1] == "128")
    );
    assert!(
        argv.windows(2)
            .any(|w| w[0] == "-v" && w[1] == "/srv/work:/work:rw")
    );
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
    for key in [
        "PATH",
        "HOME",
        "LANG",
        "TERM",
        "USER",
        "CARGO_HOME",
        "REGENT_HOME",
    ] {
        assert!(
            !is_secret_env_var(key),
            "{key} should not be treated as secret"
        );
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
