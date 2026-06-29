//! `play` — play a song/video by name. A YouTube *search* page doesn't play; the
//! *watch* URL does. So we resolve the top result with yt-dlp and open that,
//! which plays. Falls back to opening a search if yt-dlp isn't available.
//! Requires yt-dlp (`pip install yt-dlp`).

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};
use tokio::process::Command;

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
    let candidates: [&[&str]; 4] = [
        &["yt-dlp"],
        &["python", "-m", "yt_dlp"],
        &["py", "-m", "yt_dlp"],
        &["python3", "-m", "yt_dlp"],
    ];
    for base in candidates {
        let mut cmd = Command::new(base[0]);
        cmd.args(&base[1..]).args([
            "--print",
            "%(id)s\t%(title)s",
            "--no-warnings",
            "--flat-playlist",
            &search,
        ]);
        if let Ok(out) = cmd.output().await {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if let Some((id, title)) = stdout.lines().next().and_then(|l| l.split_once('\t')) {
                    if !id.trim().is_empty() {
                        return Some((id.trim().to_owned(), title.trim().to_owned()));
                    }
                }
            }
        }
    }
    None
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
