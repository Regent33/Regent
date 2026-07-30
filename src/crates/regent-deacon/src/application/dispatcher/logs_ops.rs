//! `logs.tail` — the deacon's own log file, for the workspace Output panel.
//!
//! Read-only and fixed to `$REGENT_HOME/logs/`: the request names no path, so
//! this cannot be turned into a general file reader by asking nicely. Lines are
//! already redacted on the way to disk (`infra::logging` wraps the file writer
//! in `RedactingWriter`), so what is on disk is what is safe to show.

use super::Dispatcher;
use crate::domain::entities::{RpcRequest, ok_response};
use serde_json::{Value, json};

/// How many lines a tail returns at most. Enough to cover a turn's worth of
/// activity; small enough that the panel is not asked to render a 100MB file.
const MAX_LINES: usize = 2_000;
const DEFAULT_LINES: usize = 500;

pub(super) fn tail_lines(text: &str, limit: usize) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(limit);
    lines[start..].to_vec()
}

impl Dispatcher {
    /// `logs.tail { limit? }` → `{ path, lines }`.
    ///
    /// A missing file is `{ lines: [] }`, not an error: on a fresh install
    /// nothing has been logged yet, and an error there would render as a broken
    /// panel rather than an empty one.
    pub(super) async fn logs_tail(&self, req: RpcRequest) {
        let limit = req
            .params
            .get("limit")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(DEFAULT_LINES)
            .clamp(1, MAX_LINES);

        let dir = crate::application::http_serve::regent_home().join("logs");
        let path = match newest_log(&dir).await {
            Some(path) => path,
            None => {
                self.send(ok_response(req.id, json!({ "path": null, "lines": [] })));
                return;
            }
        };
        let text = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        let lines = tail_lines(&text, limit);
        self.send(ok_response(
            req.id,
            json!({ "path": path.display().to_string(), "lines": lines }),
        ));
    }
}

/// The most recent `regent.log.<date>` in `dir`. Picked by NAME, not mtime —
/// the filenames sort chronologically by construction, and an mtime sort would
/// resurrect an old file that something else happened to touch.
async fn newest_log(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    let mut newest: Option<(String, std::path::PathBuf)> = None;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("regent.log") {
            continue;
        }
        if newest.as_ref().is_none_or(|(best, _)| name > *best) {
            newest = Some((name, entry.path()));
        }
    }
    newest.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_returns_the_last_lines_in_order() {
        let text = "a\nb\nc\nd\n";
        assert_eq!(tail_lines(text, 2), ["c", "d"]);
        // Asking for more than there is returns everything, not an error.
        assert_eq!(tail_lines(text, 99), ["a", "b", "c", "d"]);
        assert_eq!(tail_lines("", 10), Vec::<&str>::new());
    }

    /// The log file ends with a newline, so a naive split leaves a trailing
    /// empty string — which renders as a blank row that looks like a gap in
    /// the log rather than the end of it.
    #[test]
    fn blank_lines_are_dropped() {
        assert_eq!(tail_lines("a\n\n\nb\n", 10), ["a", "b"]);
    }

    #[tokio::test]
    async fn the_newest_log_is_chosen_by_name() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "regent.log.2026-07-28",
            "regent.log.2026-07-30",
            "regent.log.2026-07-29",
            "voice-server.log",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let found = newest_log(dir.path()).await.unwrap();
        assert_eq!(found.file_name().unwrap(), "regent.log.2026-07-30");
    }

    #[tokio::test]
    async fn a_missing_directory_is_none_rather_than_a_panic() {
        assert!(newest_log(std::path::Path::new("no/such/dir")).await.is_none());
    }
}
