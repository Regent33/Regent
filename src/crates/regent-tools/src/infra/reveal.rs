//! Reveal a file in the OS file manager — pop Explorer/Finder/Files with the
//! file selected, so a freshly downloaded or generated artifact is shown to the
//! user automatically. Best-effort and fire-and-forget: a failure here never
//! affects the tool that produced the file. Off with `REGENT_REVEAL_FILES=0`.

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Reveal `path` in the file manager, if revealing is enabled and not throttled.
/// Spawns the OS command and returns immediately; errors are swallowed.
pub fn reveal(path: &Path) {
    if !should_reveal() {
        return;
    }
    let _ = spawn_reveal(path);
}

/// Whether `REGENT_REVEAL_FILES` turns revealing OFF. Pure (testable); default
/// is ON (reveal), so only an explicit falsey value disables it.
fn disabled_by(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.trim(), "0" | "false" | "no" | "off"))
}

fn should_reveal() -> bool {
    if disabled_by(std::env::var("REGENT_REVEAL_FILES").ok().as_deref()) {
        return false;
    }
    // Throttle bursts: a multi-file generation shouldn't spawn one window per
    // file. ponytail: a single global 2s gate; make it per-session if a future
    // surface runs concurrent unrelated tasks.
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    if let Some(prev) = *last
        && now.duration_since(prev) < Duration::from_secs(2)
    {
        return false;
    }
    *last = Some(now);
    true
}

fn spawn_reveal(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        // `explorer /select,<path>` highlights the file. Explorer commonly exits
        // 1 even on success, so we never check status. raw_arg keeps the path's
        // quoting intact (cmd-style escaping would mangle it — see the terminal).
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("explorer");
        command.raw_arg(format!("/select,\"{}\"", path.display()));
        command.spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg("-R").arg(path).spawn().map(|_| ())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        // Prefer the FreeDesktop "show items" (selects the file in the active file
        // manager); fall back to opening the parent folder with xdg-open.
        let uri = format!("file://{}", path.display());
        let shown = Command::new("dbus-send")
            .args([
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:{uri}"),
                "string:",
            ])
            .spawn();
        if shown.is_ok() {
            return Ok(());
        }
        let dir = path.parent().unwrap_or(path);
        Command::new("xdg-open").arg(dir).spawn().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_opt_out_parsing() {
        // Default (unset) and truthy-ish values keep revealing ON.
        assert!(!disabled_by(None));
        assert!(!disabled_by(Some("1")));
        assert!(!disabled_by(Some("yes")));
        // Only explicit falsey values turn it OFF.
        for off in ["0", "false", "no", "off", " 0 "] {
            assert!(disabled_by(Some(off)), "{off:?} should disable reveal");
        }
    }
}
