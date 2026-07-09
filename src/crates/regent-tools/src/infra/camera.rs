//! `camera_capture` — grab the current camera frame so the agent can answer
//! "what am I holding right now?". Two sources, tried in order:
//! 1. The live-call frame: during a `regent call` with camera allowed, the
//!    call UI posts a frame every couple of seconds to the voice server, which
//!    writes `$REGENT_HOME/voice/camera-frame.jpg`. Fresh file → use it.
//! 2. A local webcam via `ffmpeg` (dshow/avfoundation/v4l2) when installed —
//!    covers `regent` CLI sessions outside a call.
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
    match ffmpeg_capture() {
        Ok(path) => json!({
            "success": true,
            "path": path.to_string_lossy(),
            "source": "local_webcam",
            "next_step": "call vision_analyze with this path and the user's question",
        })
        .to_string(),
        Err(reason) => tool_error_json(format!(
            "no camera frame available: {reason}. A live frame arrives automatically during a \
             `regent call` when the caller allows camera access; outside a call, installing \
             ffmpeg enables local webcam capture."
        )),
    }
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
fn ffmpeg_capture() -> Result<PathBuf, String> {
    let out = std::env::temp_dir().join("regent-camera-frame.jpg");
    let _ = std::fs::remove_file(&out);
    let args: Vec<String> = if cfg!(target_os = "windows") {
        let device = first_dshow_video_device()?;
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
    let status = std::process::Command::new("ffmpeg")
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
fn first_dshow_video_device() -> Result<String, String> {
    let output = std::process::Command::new("ffmpeg")
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
}
