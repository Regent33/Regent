//! `camera_capture` — grab the current camera frame so the agent can answer
//! "what am I holding right now?". Two sources, tried in order:
//! 1. The live-call frame: during a `regent call` with camera allowed, the
//!    call UI posts a frame every couple of seconds to the voice server, which
//!    writes `$REGENT_HOME/voice/camera-frame.jpg`. Fresh file → use it.
//! 2. A local webcam via `ffmpeg` (dshow/avfoundation/v4l2) when installed —
//!    covers `regent` CLI sessions outside a call.
//!
//! The tool returns a file path; the agent follows up with `vision_analyze`
//! (which reads local paths) to actually answer the question.

use crate::application::catalog::ToolCatalog;
use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

/// A live-call frame older than this is considered stale (the call ended or
/// the camera is off) and won't be presented as "what the user sees now".
const FRESH_FRAME_SECS: u64 = 10;
const FFMPEG_TIMEOUT_SECS: u64 = 15;

pub fn register_camera_tool(catalog: &mut ToolCatalog) -> Result<(), RegentError> {
    catalog.register(definition(), Arc::new(CameraTool))
}

fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "camera_capture".into(),
        description: "Capture the current camera/webcam frame and return its file path. Use when \
                      the user asks about what they're holding/showing/pointing the camera at \
                      ('what am I holding?', 'can you see this?'). During a live regent call with \
                      camera allowed this is the caller's camera; otherwise it captures the local \
                      webcam via ffmpeg. ALWAYS follow up with vision_analyze on the returned \
                      path, passing the user's question. For the SCREEN use computer_use \
                      screenshot instead."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
        toolset: "vision".into(),
    }
}

struct CameraTool;

#[async_trait]
impl ToolExecutor for CameraTool {
    async fn execute(&self, _args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        tokio::task::spawn_blocking(|| Ok(capture()))
            .await
            .map_err(|e| RegentError::Tool {
                tool: "camera_capture".into(),
                message: e.to_string(),
            })?
    }
}

/// `$REGENT_HOME/voice/camera-frame.jpg` — written by the voice server's
/// `/call/frame` route while a call with camera runs.
fn live_frame_path() -> Option<PathBuf> {
    let home = std::env::var("REGENT_HOME").ok()?;
    Some(PathBuf::from(home).join("voice").join("camera-frame.jpg"))
}

fn frame_age_secs(path: &PathBuf) -> Option<u64> {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|age| age.as_secs())
}

fn capture() -> String {
    if let Some(result) = live_frame_result(live_frame_path()) {
        return result;
    }
    let Some(ffmpeg) = resolve_ffmpeg() else {
        return tool_error_json(format!(
            "no camera frame available: local webcam capture needs ffmpeg, which isn't \
             installed or on PATH. Install it and try again — {}. (During a `regent call` with \
             camera allowed, a frame arrives automatically and needs no ffmpeg.)",
            install_hint()
        ));
    };
    match ffmpeg_capture(&ffmpeg) {
        Ok(path) => json!({
            "success": true,
            "path": path.to_string_lossy(),
            "source": "local_webcam",
            "next_step": "call vision_analyze with this path and the user's question",
        })
        .to_string(),
        Err(reason) => tool_error_json(format!(
            "no camera frame available: {reason}. A live frame arrives automatically during a \
             `regent call` when the caller allows camera access; outside a call, a working webcam \
             + ffmpeg is required ({}).",
            install_hint()
        )),
    }
}

/// The platform's one-liner for getting ffmpeg, so the error is copy-pasteable.
fn install_hint() -> &'static str {
    if cfg!(target_os = "windows") {
        "install with `winget install Gyan.FFmpeg` (or set REGENT_FFMPEG to an ffmpeg.exe path)"
    } else if cfg!(target_os = "macos") {
        "install with `brew install ffmpeg`"
    } else {
        "install with your package manager, e.g. `sudo apt install ffmpeg`"
    }
}

/// Locate an ffmpeg binary. A detached gateway inherits a trimmed PATH, so a
/// bare `ffmpeg` lookup misses installs that ARE present — check the common
/// install locations and a `REGENT_FFMPEG` override before giving up. Returns
/// `None` only when nothing resolves (so the caller shows the install hint
/// instead of a raw spawn error).
fn resolve_ffmpeg() -> Option<std::ffi::OsString> {
    let exe = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    // 1. Explicit override wins.
    if let Ok(path) = std::env::var("REGENT_FFMPEG")
        && !path.is_empty()
        && std::path::Path::new(&path).exists()
    {
        return Some(path.into());
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    // 2. Beside the running binary — the installer drops ffmpeg into the same
    //    bin dir as regent-deacon/regent-gateway, whatever the install location
    //    (GUI installs can pick a custom dir). This is the install-location-
    //    independent hit, so it comes first.
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(dir) = exe_path.parent()
    {
        candidates.push(dir.join(exe));
    }
    // 3. A portable build under $REGENT_HOME/bin (the one-liner install target).
    if let Ok(home) = std::env::var("REGENT_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(exe));
    }
    // 4. Common Windows install locations a trimmed PATH tends to miss.
    if cfg!(target_os = "windows")
        && let Ok(user) = std::env::var("USERPROFILE")
    {
        candidates.push(
            PathBuf::from(&user)
                .join("AppData/Local/Microsoft/WinGet/Links")
                .join(exe),
        );
        candidates.push(PathBuf::from(&user).join("scoop/shims").join(exe));
        candidates.push(PathBuf::from(r"C:\Program Files\ffmpeg\bin").join(exe));
        candidates.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin").join(exe));
    }
    if let Some(hit) = candidates.into_iter().find(|c| c.exists()) {
        return Some(hit.into_os_string());
    }
    // 5. Fall back to the bare name so a normal PATH still works; if it isn't
    //    there either, the spawn fails and the caller reports it. `which`-style
    //    probe keeps the None path honest (no ffmpeg anywhere we can see).
    on_path(exe).then(|| exe.into())
}

/// Whether a bare command resolves on PATH (so a missing ffmpeg reports as
/// "not installed" rather than a confusing spawn error).
fn on_path(exe: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(exe).exists())
}

/// The live-call frame as a tool result, if one exists and is fresh.
fn live_frame_result(path: Option<PathBuf>) -> Option<String> {
    let path = path?;
    let age = frame_age_secs(&path)?;
    (age <= FRESH_FRAME_SECS).then(|| {
        json!({
            "success": true,
            "path": path.to_string_lossy(),
            "source": "live_call_camera",
            "next_step": "call vision_analyze with this path and the user's question",
        })
        .to_string()
    })
}

/// One-frame webcam grab via ffmpeg, platform-native capture backends.
/// ponytail: shells out to ffmpeg instead of a webcam crate — no new deps,
/// and the live-call path (no ffmpeg needed) is the primary source.
fn ffmpeg_capture(ffmpeg: &std::ffi::OsString) -> Result<PathBuf, String> {
    let out = std::env::temp_dir().join("regent-camera-frame.jpg");
    let _ = std::fs::remove_file(&out);
    let args: Vec<String> = if cfg!(target_os = "windows") {
        let device = first_dshow_video_device(ffmpeg)?;
        vec![
            "-f".into(),
            "dshow".into(),
            "-i".into(),
            format!("video={device}"),
        ]
    } else if cfg!(target_os = "macos") {
        vec!["-f".into(), "avfoundation".into(), "-i".into(), "0".into()]
    } else {
        vec![
            "-f".into(),
            "v4l2".into(),
            "-i".into(),
            "/dev/video0".into(),
        ]
    };
    let status = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(&args)
        .args(["-frames:v", "1", "-t", &FFMPEG_TIMEOUT_SECS.to_string()])
        .arg(&out)
        .output()
        .map_err(|e| format!("ffmpeg not runnable ({e})"))?;
    if !status.status.success() || !out.exists() {
        return Err(format!(
            "ffmpeg capture failed: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        ));
    }
    Ok(out)
}

/// First DirectShow video device name (Windows), from ffmpeg's device list.
fn first_dshow_video_device(ffmpeg: &std::ffi::OsString) -> Result<String, String> {
    let output = std::process::Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-list_devices",
            "true",
            "-f",
            "dshow",
            "-i",
            "dummy",
        ])
        .output()
        .map_err(|e| format!("ffmpeg not runnable ({e})"))?;
    // Device list goes to stderr: `"Device Name" (video)` lines.
    let listing = String::from_utf8_lossy(&output.stderr);
    listing
        .lines()
        .filter(|l| l.contains("(video)"))
        .filter_map(|l| {
            let start = l.find('"')? + 1;
            let end = l[start..].find('"')? + start;
            Some(l[start..end].to_owned())
        })
        .next()
        .ok_or_else(|| "no webcam device found".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh live-call frame is returned directly (no ffmpeg involved);
    /// a missing frame falls through to the capture path.
    #[test]
    fn fresh_live_frame_wins() {
        let dir = tempfile::tempdir().unwrap();
        let frame = dir.path().join("camera-frame.jpg");
        std::fs::write(&frame, [0xff, 0xd8, 0xff, 0xd9]).unwrap();

        let reply = live_frame_result(Some(frame)).expect("fresh frame is used");
        assert!(reply.contains("live_call_camera"), "got: {reply}");
        assert!(reply.contains("vision_analyze"));

        assert!(live_frame_result(Some(dir.path().join("missing.jpg"))).is_none());
        assert!(live_frame_result(None).is_none());
    }

    /// The `REGENT_FFMPEG` override points capture at a specific binary; a
    /// non-existent override is ignored (so it can't mask a real ffmpeg on
    /// PATH). REGENT_FFMPEG is only touched here, so no cross-test race.
    #[test]
    fn explicit_ffmpeg_override_is_honoured_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("ffmpeg.exe");
        std::fs::write(&fake, b"x").unwrap();

        unsafe { std::env::set_var("REGENT_FFMPEG", &fake) };
        let resolved = resolve_ffmpeg().expect("override resolves");
        assert_eq!(std::path::Path::new(&resolved), fake);

        unsafe { std::env::set_var("REGENT_FFMPEG", "Z:\\does\\not\\exist\\ffmpeg.exe") };
        assert_ne!(
            resolve_ffmpeg().as_deref(),
            Some(std::ffi::OsStr::new("Z:\\does\\not\\exist\\ffmpeg.exe")),
            "a bogus override must not be returned"
        );
        unsafe { std::env::remove_var("REGENT_FFMPEG") };

        assert!(!install_hint().is_empty());
    }
}
