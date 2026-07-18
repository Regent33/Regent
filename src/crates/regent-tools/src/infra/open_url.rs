//! `open_url` — open a website in the user's default browser (a bare domain is
//! upgraded to https first). URLs only: launching an arbitrary local path from
//! an unapproved, voice-reachable tool would be a foot-gun, so files go through
//! `reveal`/`terminal` (gated). The OS launcher [`open`] is shared with `play`.

use crate::domain::contracts::ToolExecutor;
use crate::domain::entities::ToolContext;
use async_trait::async_trait;
use regent_kernel::{RegentError, ToolDefinition, tool_error_json};
use serde_json::{Value, json};

#[must_use]
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "open_url".into(),
        description: "Open a website in the user's default browser — use this for 'pull up \
                      <site>', 'open <url>', 'go to <website>'. Accepts a full URL or a bare \
                      domain like 'facebook.com' (opened as https). Opens on the user's screen and \
                      returns immediately. (For a song or video use `play`; for a local file use \
                      the terminal launcher.)"
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "A website URL or bare domain, e.g. 'https://facebook.com' or 'facebook.com'."
                }
            },
            "required": ["target"]
        }),
        toolset: "web".into(),
    }
}

pub struct OpenUrlTool;

#[async_trait]
impl ToolExecutor for OpenUrlTool {
    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<String, RegentError> {
        let target = args.get("target").and_then(Value::as_str).unwrap_or("");
        match normalize_url(target) {
            Some(url) => {
                open(&url);
                Ok(json!({ "opened": url }).to_string())
            }
            None => Ok(tool_error_json(
                "open_url takes a website URL or bare domain (e.g. 'facebook.com'); for a local \
                 file use the terminal launcher, and for a song or video use play",
            )),
        }
    }
}

/// Accept an http(s) URL as-is, upgrade a bare domain ("facebook.com") to
/// https, and reject everything else (other schemes, local paths, junk) — so an
/// unapproved, voice-reachable tool can never launch an arbitrary local program.
/// ponytail: URLs only; files go through the gated reveal/terminal path.
fn normalize_url(target: &str) -> Option<String> {
    let t = target.trim();
    // SECURITY: this tool is unapproved and voice-reachable, so `target` is
    // untrusted. It ends up inside `cmd /c start "" "<url>"` — a `"` would break
    // out of the quotes (command injection) and a `%` would trigger cmd env-var
    // expansion (info leak). Reject those, whitespace, and control chars up front
    // so only a clean URL ever reaches the launcher. (Rules out %-encoded URLs —
    // fine for a "pull up <site>" affordance.)
    if t.is_empty()
        || t.chars()
            .any(|c| c == '"' || c == '%' || c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Some(t.to_owned())
    } else if !t.contains(':') && t.contains('.') {
        Some(format!("https://{t}"))
    } else {
        None
    }
}

/// Launch `url` in the OS default handler (the browser, for http/https). Shared
/// with the `play` tool. Fire-and-forget: a spawn error is ignored (nothing to
/// recover, and the caller already returned success to the model).
pub(crate) fn open(url: &str) {
    #[cfg(windows)]
    {
        // `explorer.exe <url>` mis-parses a query string (`?`/`&`) and opens a
        // FOLDER; `start` hands the URL to the default browser. raw_arg keeps it
        // quoted so an `&` isn't split as a cmd separator.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000; // no console flash
        let _ = std::process::Command::new("cmd")
            .raw_arg(format!("/c start \"\" \"{url}\""))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_url;

    #[test]
    fn accepts_urls_and_bare_domains_rejects_local_paths_and_schemes() {
        assert_eq!(
            normalize_url("https://facebook.com").as_deref(),
            Some("https://facebook.com")
        );
        assert_eq!(
            normalize_url("facebook.com").as_deref(),
            Some("https://facebook.com")
        );
        assert_eq!(
            normalize_url("example.com/page").as_deref(),
            Some("https://example.com/page")
        );
        // Other schemes / local paths / junk are refused — no unapproved exec.
        assert_eq!(normalize_url("file:///etc/passwd"), None);
        assert_eq!(normalize_url("C:\\Windows\\system32\\calc.exe"), None);
        assert_eq!(normalize_url("mailto:a@b.com"), None);
        assert_eq!(normalize_url("just some words"), None);
        assert_eq!(normalize_url(""), None);
    }

    #[test]
    fn rejects_cmd_injection_and_env_expansion_payloads() {
        // A `"` would break out of the `start "" "<url>"` quoting; `%` expands a
        // cmd env var; both must be refused even behind an http(s) prefix.
        assert_eq!(normalize_url("https://x.com/\"&calc.exe&\""), None);
        assert_eq!(normalize_url("https://x.com/%USERPROFILE%"), None);
        assert_eq!(normalize_url("https://x.com/a b"), None); // whitespace
    }
}
