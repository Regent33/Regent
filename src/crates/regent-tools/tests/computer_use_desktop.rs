//! Does computer-use ACTUALLY drive this desktop? Everything else about the
//! tool is covered by unit tests — parsing, the approval gate, the shape of the
//! generated scripts — but none of that proves a keystroke reaches a window.
//! This drives the real `PowerShellBackend` against a real Notepad and reads
//! the text back out of it.
//!
//! `#[ignore]`d on purpose: it needs an interactive desktop session (SendKeys
//! goes to whatever holds focus), so it is meaningless on a CI runner and
//! disruptive on a machine someone is using. Run it deliberately:
//!
//!   cargo test -p regent-tools --test computer_use_desktop -- --ignored --nocapture
//!
//! Two safety rules, both learned the hard way while writing it:
//!
//!   * It types into a window it CREATED, identified by a unique temp-file
//!     title. Windows 11 Notepad is a packaged app, so `Start-Process -PassThru`
//!     returns a stub pid and the window belongs to another process — matching
//!     on "the Notepad process" can land on a Notepad the user already had
//!     open, and this test types and then selects-all.
//!   * It refuses to type at all unless the foreground window is confirmed to
//!     be that window. SendKeys goes to whatever has focus; a missed focus
//!     types into someone else's editor.
#![cfg(windows)]

use regent_tools::infra::computer_use::{Action, ComputerBackend, PowerShellBackend};
use std::process::Command;
use std::time::Duration;

fn powershell(script: &str) -> String {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .expect("powershell runs");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

async fn act(backend: &PowerShellBackend, action: Action) -> String {
    backend
        .act(&action)
        .await
        .unwrap_or_else(|e| panic!("{action:?} failed: {e}"))
        .note
}

/// The `window_id` of the listed window whose title contains `needle`.
/// `window_id` is NOT the process id — the listing carries both, and passing a
/// pid where a window id belongs simply finds nothing.
fn window_id_titled(listing: &str, needle: &str) -> Option<i64> {
    let rows: serde_json::Value = serde_json::from_str(listing).ok()?;
    rows.as_array()?.iter().find_map(|row| {
        row.get("title")?
            .as_str()?
            .contains(needle)
            .then(|| row.get("window_id")?.as_i64())?
    })
}

#[tokio::test]
#[ignore = "drives the real desktop; run explicitly with --ignored"]
async fn types_into_a_real_window_and_the_text_is_actually_there() {
    let backend = PowerShellBackend;

    // A uniquely named scratch file, so the window this test drives cannot be
    // confused with anything the user already has open.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let name = format!("regent-cu-{stamp}");
    let path = std::env::temp_dir().join(format!("{name}.txt"));
    std::fs::write(&path, "").expect("scratch file");
    let cleanup = || {
        let _ = powershell(&format!(
            "Get-Process notepad -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowTitle -like '*{name}*' }} | Stop-Process -Force"
        ));
        let _ = std::fs::remove_file(&path);
    };

    powershell(&format!(
        "Start-Process notepad -ArgumentList '{}' | Out-Null",
        path.display()
    ));
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let listed = act(&backend, Action::ListWindows).await;
    let Some(window_id) = window_id_titled(&listed, &name) else {
        cleanup();
        panic!("the scratch window '{name}' never appeared in list_windows:\n{listed}");
    };

    act(&backend, Action::FocusWindow { window_id }).await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Never type blind. If focus did not land, stop here rather than send
    // keystrokes into whatever window happens to be in front.
    let front = powershell(
        "Add-Type @\"\nusing System;using System.Runtime.InteropServices;using System.Text;\npublic class Fg { [DllImport(\"user32.dll\")] public static extern IntPtr GetForegroundWindow(); [DllImport(\"user32.dll\")] public static extern int GetWindowText(IntPtr h,StringBuilder s,int n); }\n\"@; $sb=New-Object System.Text.StringBuilder 512; [void][Fg]::GetWindowText([Fg]::GetForegroundWindow(),$sb,512); $sb.ToString()",
    );
    if !front.contains(&name) {
        cleanup();
        panic!(
            "focus_window did not bring the scratch window forward (front window is {front:?}) — refusing to type"
        );
    }

    // Deliberately includes SendKeys metacharacters: `escape_sendkeys` has to
    // send them literally, and a bare `+` or `%` reaching SendKeys unescaped
    // would silently become Shift or Alt.
    let phrase = "regent types here 100% (+ok)";
    act(
        &backend,
        Action::Type {
            text: phrase.to_owned(),
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(900)).await;

    // Read it back THROUGH the same input path: select-all and copy are Key
    // combos, so passing here exercises Key as well as Type.
    for combo in ["ctrl+a", "ctrl+c"] {
        act(
            &backend,
            Action::Key {
                combo: combo.to_owned(),
            },
        )
        .await;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    let clipboard = powershell("Get-Clipboard -Raw");
    cleanup();

    assert_eq!(
        clipboard.trim(),
        phrase,
        "what landed in the window is not what was typed"
    );
}
