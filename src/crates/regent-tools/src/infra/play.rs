//! `play` — play a song/video by name. A YouTube *search* page doesn't play; the
//! *watch* URL does. So we resolve the top result with yt-dlp and open that,
//! which plays. Falls back to opening a search if yt-dlp isn't available.
//! Requires yt-dlp (`pip install yt-dlp`).

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::process::Command;

/// Cap each yt-dlp resolve so a stalled/throttled yt-dlp can't hang the whole
/// turn ("stuck on thinking"). On timeout we fall back to opening a search.
const RESOLVE_TIMEOUT_SECS: u64 = 15;

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "play".into(),
        description: "Play a song or video by name. Resolves the top YouTube result and opens it \
                      PLAYING in the browser — use this for 'play <song>' / 'put on <artist>' \
                      requests instead of opening a search results page."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to play, e.g. 'AC/DC Thunderstruck'."
                }
            },
            "required": ["query"]
        }),
        toolset: "media".into(),
    }
}

pub struct PlayTool;

#[async_trait]
impl ToolExecutor for PlayTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let Some(query) = args
            .get("query")
            .and_then(Value::as_str)
            .filter(|q| !q.trim().is_empty())
        else {
            return Ok(tool_error_json("missing required parameter: query"));
        };
        if let Some((id, title)) = resolve_video(query).await {
            let url = format!("https://www.youtube.com/watch?v={id}");
            open_url(&url);
            Ok(json!({ "playing": title, "url": url }).to_string())
        } else {
            // yt-dlp missing/failed → open a search so the user can pick.
            let url = format!("https://www.youtube.com/results?search_query={}", url_encode(query));
            open_url(&url);
            Ok(json!({
                "note": "couldn't resolve the top result (is yt-dlp installed?) — opened a search instead",
                "url": url
            })
            .to_string())
        }
    }
}

/// Resolve the top YouTube result to `(id, title)`, trying a few ways to invoke
/// yt-dlp (PATH, then `python -m yt_dlp`) so it works without yt-dlp on PATH.
async fn resolve_video(query: &str) -> Option<(String, String)> {
    let search = format!("ytsearch1:{query}");
    // Invocations, best first. The daemon's PATH often lacks the pip user-install
    // Scripts dir, and `py`/`python` may point at a different interpreter without
    // the yt_dlp module — so try a discovered absolute yt-dlp path first, then
    // PATH, then the module forms.
    let mut invocations: Vec<Vec<String>> = Vec::new();
    if let Some(full) = discover_yt_dlp() {
        invocations.push(vec![full]);
    }
    for base in [
        vec!["yt-dlp".to_owned()],
        vec!["python".to_owned(), "-m".to_owned(), "yt_dlp".to_owned()],
        vec!["py".to_owned(), "-m".to_owned(), "yt_dlp".to_owned()],
        vec!["python3".to_owned(), "-m".to_owned(), "yt_dlp".to_owned()],
    ] {
        invocations.push(base);
    }

    for inv in &invocations {
        let mut cmd = Command::new(&inv[0]);
        cmd.args(&inv[1..]).args([
            "--print",
            "%(id)s\t%(title)s",
            "--no-warnings",
            "--flat-playlist",
            &search,
        ]);
        cmd.kill_on_drop(true); // a timed-out resolve is killed, not orphaned
        match tokio::time::timeout(Duration::from_secs(RESOLVE_TIMEOUT_SECS), cmd.output()).await {
            // Present but stalled → stop and let the caller fall back to a search,
            // rather than hang (and rather than retry the other invocations).
            Err(_) => return None,
            // Not installed (spawn failed) → try the next invocation.
            Ok(Err(_)) => continue,
            Ok(Ok(out)) => {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    if let Some((id, title)) =
                        stdout.lines().next().and_then(|l| l.split_once('\t'))
                        && !id.trim().is_empty()
                    {
                        return Some((id.trim().to_owned(), title.trim().to_owned()));
                    }
                }
                // Ran but produced nothing usable (e.g. `py` without the module) →
                // keep trying the remaining invocations.
            }
        }
    }
    None
}

/// Find a `yt-dlp` executable when it isn't on the daemon's PATH — common pip
/// user-install Scripts dirs on Windows, well-known bin dirs elsewhere. `None`
/// when not found (the caller then tries PATH / `python -m yt_dlp`).
fn discover_yt_dlp() -> Option<String> {
    #[cfg(windows)]
    {
        let exe = "yt-dlp.exe";
        for root_var in ["LOCALAPPDATA", "APPDATA"] {
            let Ok(root) = std::env::var(root_var) else {
                continue;
            };
            // pip installs land in <root>\Python\<tag>\Scripts\yt-dlp.exe.
            let py = std::path::Path::new(&root).join("Python");
            if let Ok(entries) = std::fs::read_dir(&py) {
                for entry in entries.flatten() {
                    let cand = entry.path().join("Scripts").join(exe);
                    if cand.is_file() {
                        return Some(cand.to_string_lossy().into_owned());
                    }
                }
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        let mut cands = vec![
            "/usr/local/bin/yt-dlp".to_owned(),
            "/opt/homebrew/bin/yt-dlp".to_owned(),
        ];
        if let Ok(home) = std::env::var("HOME") {
            cands.push(format!("{home}/.local/bin/yt-dlp"));
        }
        cands.into_iter().find(|p| std::path::Path::new(p).is_file())
    }
}

fn open_url(url: &str) {
    let _ = if cfg!(windows) {
        std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
}

/// Minimal percent-encoding for the search fallback query.
fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_owned(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
