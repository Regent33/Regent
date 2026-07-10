//! Spawn `regent-voice-server` for Butler Mode — detached, hidden, and REUSED
//! across app runs (the same semantics as `regent call`'s voiceServe.ts: probe
//! first, spawn only when down, never kill on exit). The webview owns the
//! `:8000/health` probe (its CSP already allows that origin); this command only
//! launches the process. Stale-binary caveat applies (see CHANGELOG 2026-07-06):
//! after voice-server changes, rebuild release AND kill the running process.

use crate::deacon::{merged_env, regent_home};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Launch the voice server if a binary can be found. Returns the path used, so
/// the UI can show what it started. Idempotence is the caller's job (probe
/// /health first) — a second spawn on a bound port exits on its own.
#[tauri::command]
pub fn voice_spawn() -> Result<String, String> {
    let (bin, cwd) = find_voice_server().ok_or_else(|| {
        "regent-voice-server binary not found (set REGENT_VOICE_SERVER_PATH or build it \
         with `cargo build -p regent-voice-server --release`)"
            .to_string()
    })?;
    let home = regent_home();
    std::fs::create_dir_all(&home)
        .map_err(|e| format!("create REGENT_HOME {}: {e}", home.display()))?;

    let mut cmd = Command::new(&bin);
    // cwd = the target/ dir's parent so the default models dir
    // (tts-asr-local-models) resolves at the repo root, like voiceServe.ts.
    cmd.current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .envs(merged_env(&home));
    // The server's CORS is deny-by-default with exactly one configurable extra
    // origin — grant the packaged webview (dev runs on :3000, already allowed).
    // An explicitly-set real env still wins.
    if std::env::var("REGENT_CALL_UI_ORIGIN").is_err() {
        cmd.env("REGENT_CALL_UI_ORIGIN", "http://tauri.localhost");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP: hidden (a visible console
        // invites closing it, killing the voice mid-call) and detached from our
        // ctrl-c group so it survives this app exiting.
        cmd.creation_flags(0x0800_0000 | 0x0000_0200);
    }
    cmd.spawn()
        .map(|_| bin.display().to_string()) // handle dropped → child keeps running
        .map_err(|e| format!("spawn voice server {}: {e}", bin.display()))
}

fn server_name() -> &'static str {
    if cfg!(windows) {
        "regent-voice-server.exe"
    } else {
        "regent-voice-server"
    }
}

/// `REGENT_VOICE_SERVER_PATH` override, then `target/{release,debug}` walking
/// up from the cwd and this exe — the same walk as `find_deacon`, returning
/// the base dir as cwd for the models-dir default.
fn find_voice_server() -> Option<(PathBuf, PathBuf)> {
    if let Ok(p) = std::env::var("REGENT_VOICE_SERVER_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            let cwd = p.parent().map(PathBuf::from).unwrap_or_default();
            return Some((p, cwd));
        }
    }
    let name = server_name();
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(cwd.ancestors().map(PathBuf::from));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            bases.extend(dir.ancestors().map(PathBuf::from));
        }
    }
    for base in &bases {
        // Newest of release/debug wins — same staleness rule as find_deacon.
        if let Some(cand) = crate::deacon::newest_in_target(base, name) {
            return Some((cand, base.clone()));
        }
    }
    None
}
