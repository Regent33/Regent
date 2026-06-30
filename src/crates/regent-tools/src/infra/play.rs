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
            let url = format!(
                "https://www.youtube.com/results?search_query={}",
                url_encode(query)
            );
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
    // Search the top few and rank, rather than blindly taking #1 (which is often a
    // lyric video, cover, or live cut) — see `pick_best`.
    let search = format!("ytsearch5:{query}");
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
            "%(id)s\t%(title)s\t%(channel)s\t%(view_count)s",
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
                if out.status.success()
                    && let Some(best) = pick_best(&String::from_utf8_lossy(&out.stdout), query)
                {
                    return Some(best);
                }
                // Ran but produced nothing usable (e.g. `py` without the module) →
                // keep trying the remaining invocations.
            }
        }
    }
    None
}

/// Rank yt-dlp search rows (`id\ttitle\tchannel\tview_count`) and pick the most
/// likely "the actual song": prefer the official upload (title says "official",
/// or a VEVO / "- Topic" / official channel) and higher view counts, while
/// down-ranking live/cover/lyric/remix cuts the user didn't ask for. If the
/// query itself names a variant (e.g. "live"), keep only those.
fn pick_best(stdout: &str, query: &str) -> Option<(String, String)> {
    const BAD: &[&str] = &[
        "live",
        "cover",
        "reaction",
        "remix",
        "nightcore",
        "sped up",
        "slowed",
        "8d",
        "instrumental",
        "karaoke",
        "tutorial",
        "lyric",
    ];
    let ql = query.to_lowercase();

    struct Cand {
        id: String,
        title: String,
        tl: String,
        cl: String,
        views: f64,
    }
    let mut cands: Vec<Cand> = Vec::new();
    for line in stdout.lines() {
        let mut p = line.splitn(4, '\t');
        let (Some(id), Some(title)) = (p.next(), p.next()) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        let channel = p.next().unwrap_or("");
        let views = p
            .next()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        cands.push(Cand {
            id: id.to_owned(),
            title: title.trim().to_owned(),
            tl: title.to_lowercase(),
            cl: channel.to_lowercase(),
            views,
        });
    }
    if cands.is_empty() {
        return None;
    }

    // If the user explicitly asked for a variant, restrict to matches (intent
    // beats popularity); otherwise consider everything.
    let requested: Vec<&str> = BAD.iter().copied().filter(|b| ql.contains(b)).collect();
    let pool: Vec<&Cand> = if requested.is_empty() {
        cands.iter().collect()
    } else {
        let matches: Vec<&Cand> = cands
            .iter()
            .filter(|c| requested.iter().any(|q| c.tl.contains(q)))
            .collect();
        if matches.is_empty() {
            cands.iter().collect()
        } else {
            matches
        }
    };

    let score = |c: &Cand| -> f64 {
        let mut s = c.views.max(1.0);
        if BAD.iter().any(|b| c.tl.contains(b) && !ql.contains(b)) {
            s *= 0.02; // a variant the user didn't ask for
        }
        if c.tl.contains("official")
            || c.cl.contains("vevo")
            || c.cl.contains("official")
            || c.cl.ends_with("- topic")
        {
            s *= 3.0; // official upload
        }
        s
    };
    pool.into_iter()
        .max_by(|a, b| {
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|c| (c.id.clone(), c.title.clone()))
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
        cands
            .into_iter()
            .find(|p| std::path::Path::new(p).is_file())
    }
}

fn open_url(url: &str) {
    let _ = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_official_over_live_lyrics_cover() {
        let out = "ID1\tAC/DC - Back In Black (Official 4K Video)\tAC/DC\t1214313302\n\
                   ID2\tAC/DC - Back In Black (Lyrics)\t7clouds Rock\t9142071\n\
                   ID3\tAC/DC - Back In Black (Live At River Plate)\tAC/DC\t71434597\n\
                   ID4\tBack In Black cover\tSome Band\t500000";
        assert_eq!(pick_best(out, "back in black acdc").unwrap().0, "ID1");
    }

    #[test]
    fn respects_an_explicit_live_request() {
        // Intent beats popularity: the studio cut has 200x the views, but the
        // user asked for "live", so the live row wins.
        let out = "ID1\tSong (Official Video)\tArtistVEVO\t1000000000\n\
                   ID2\tSong (Live at Wembley)\tArtist\t5000000";
        assert_eq!(pick_best(out, "song live").unwrap().0, "ID2");
    }

    #[test]
    fn none_when_no_rows() {
        assert!(pick_best("", "anything").is_none());
    }
}
