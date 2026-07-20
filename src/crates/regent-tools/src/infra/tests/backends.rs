//! Unit tests for `backends` (extracted for the file-size rule; same
//! module tree via #[path] — `use super::*` still sees the parent).

use super::*;

// Regression: a quoted Windows command must reach cmd.exe verbatim, not get
// mangled into backslashes by Rust's arg escaping (the `start "" "url"` →
// `\\` bug). cmd echoes the quotes literally; what matters is no stray `\`.
#[cfg(windows)]
#[tokio::test]
async fn windows_quoted_command_is_not_mangled() {
    let out = LocalBackend
        .run(
            "echo \"hello world\"",
            &std::env::temp_dir(),
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert_eq!(out.exit_code, Some(0));
    assert!(out.stdout.contains("hello world"), "stdout: {}", out.stdout);
    assert!(
        !out.stdout.contains('\\'),
        "quotes mangled to backslashes: {}",
        out.stdout
    );
}

#[test]
fn docker_and_ssh_argv_shapes() {
    assert_eq!(
        build_docker_args("dev", Some("/work"), "echo hi"),
        [
            "docker", "exec", "-w", "/work", "dev", "sh", "-c", "echo hi"
        ]
    );
    assert_eq!(
        build_docker_args("dev", None, "ls"),
        ["docker", "exec", "dev", "sh", "-c", "ls"]
    );
    assert_eq!(
        build_ssh_args("me@box", "uptime"),
        ["ssh", "-o", "BatchMode=yes", "me@box", "uptime"]
    );
}

#[test]
fn backend_env_parsing() {
    assert_eq!(parse_backend("local").unwrap().describe(), "local");
    assert_eq!(
        parse_backend("docker:dev").unwrap().describe(),
        "docker:dev"
    );
    assert_eq!(
        parse_backend("docker:dev:/srv").unwrap().describe(),
        "docker:dev"
    );
    assert_eq!(
        parse_backend("ssh:me@box").unwrap().describe(),
        "ssh:me@box"
    );
    assert!(parse_backend("kubernetes:pod").is_err());
}
