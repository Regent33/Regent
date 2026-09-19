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

/// Poll `list_windows` until a window titled with `needle` exists (a cold
/// start of packaged Notepad can take several seconds; a fixed sleep flaked).
async fn wait_for_window(backend: &PowerShellBackend, needle: &str) -> Result<i64, String> {
    let mut listed = String::new();
    for _ in 0..40 {
        listed = act(backend, Action::ListWindows).await;
        if let Some(id) = window_id_titled(&listed, needle) {
            return Ok(id);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(listed)
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
    let backend = PowerShellBackend::default();

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
    let window_id = match wait_for_window(&backend, &name).await {
        Ok(id) => id,
        Err(listed) => {
            cleanup();
            panic!("the scratch window '{name}' never appeared in list_windows:\n{listed}");
        }
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

/// Open a uniquely titled Notepad and return (its window_id, a cleanup fn).
async fn scratch_notepad(backend: &PowerShellBackend, tag: &str) -> (i64, impl Fn()) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let name = format!("regent-cu-{tag}-{stamp}");
    let path = std::env::temp_dir().join(format!("{name}.txt"));
    std::fs::write(&path, "").expect("scratch file");
    powershell(&format!(
        "Start-Process notepad -ArgumentList '{}' | Out-Null",
        path.display()
    ));
    let window_id = wait_for_window(backend, &name)
        .await
        .unwrap_or_else(|listed| panic!("'{name}' never appeared in list_windows:\n{listed}"));
    let cleanup = move || {
        let _ = powershell(&format!(
            "Get-Process notepad -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowTitle -like '*{name}*' }} | Stop-Process -Force"
        ));
        let _ = std::fs::remove_file(&path);
    };
    (window_id, cleanup)
}

/// The user outranks the agent: typing pinned to window A must stop the
/// moment another app comes forward, and must refuse outright when the pinned
/// target is not in front to begin with.
#[tokio::test]
#[ignore = "drives the real desktop; run explicitly with --ignored"]
async fn typing_stops_when_the_user_brings_another_window_forward() {
    let backend = PowerShellBackend::default();
    let (a, cleanup_a) = scratch_notepad(&backend, "a").await;
    // A second app entirely: the pin is per process, so a sibling Notepad
    // window would still count as "the same app". Paint is packaged Win32 and
    // owns its window (Calculator is UWP: its window belongs to
    // ApplicationFrameHost and cannot be activated by pid).
    powershell("Start-Process mspaint | Out-Null");
    let paint = wait_for_window(&backend, "Paint")
        .await
        .unwrap_or_else(|listed| {
            panic!(
                "Paint never appeared:
{listed}"
            )
        });
    let cleanup_b = || {
        let _ =
            powershell("Get-Process mspaint -ErrorAction SilentlyContinue | Stop-Process -Force");
    };

    act(&backend, Action::FocusWindow { window_id: a }).await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    // ~3.2k chars = 80 chunks, several seconds of typing; bring Paint forward
    // after 1.5s the way a user would (the same foreground-lock-safe route the
    // backend uses, but NOT through the backend, so the pin stays on Notepad).
    let text = "regent keeps typing until the user takes over ".repeat(70);
    let takeover = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        powershell(&format!(
            "Add-Type @\"
using System;using System.Runtime.InteropServices;
public class W {{ [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern IntPtr GetForegroundWindow(); [DllImport(\"user32.dll\")] public static extern uint GetWindowThreadProcessId(IntPtr h, out int pid); [DllImport(\"user32.dll\")] public static extern bool AttachThreadInput(uint a,uint b,bool attach); [DllImport(\"kernel32.dll\")] public static extern uint GetCurrentThreadId(); }}
\"@;              $d=0; $fg=[W]::GetWindowThreadProcessId([W]::GetForegroundWindow(),[ref]$d); $me=[W]::GetCurrentThreadId();              [void][W]::AttachThreadInput($me,$fg,$true); [void][W]::SetForegroundWindow([IntPtr]{paint}); [void][W]::AttachThreadInput($me,$fg,$false)"
        ));
    });
    let result = backend.act(&Action::Type { text }).await;
    let _ = takeover.await;
    let err = match result {
        Ok(out) => {
            cleanup_a();
            cleanup_b();
            panic!(
                "typing ran to completion despite the takeover: {}",
                out.note
            );
        }
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("typing stopped after"),
        "expected the foreground guard to stop typing, got: {err}"
    );

    // Paint is still in front: a pinned action must refuse up front.
    let refused = backend
        .act(&Action::Type {
            text: "must not land".into(),
        })
        .await
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    cleanup_a();
    cleanup_b();
    assert!(
        refused.contains("not in the foreground"),
        "expected the pin to refuse, got: {refused}"
    );
}

/// The global emergency stop: the chord (even injected — hotkeys fire on
/// synthetic input) latches `halted`, while the same injected key events do
/// NOT count as human input (the hooks read the INJECTED flag).
#[tokio::test]
#[ignore = "registers a global hotkey and injects keys; run explicitly with --ignored"]
async fn emergency_stop_chord_latches_and_injected_keys_are_not_human() {
    use regent_tools::infra::computer_use::human;
    human::start();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let before = human::last_human_input_ms();
    // Ctrl(0x11)+Alt(0x12)+Shift(0x10)+Esc(0x1B) via keybd_event, 40ms apart.
    powershell(
        "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\npublic class K { [DllImport(\"user32.dll\")] public static extern void keybd_event(byte vk,byte scan,uint flags,IntPtr extra); }\n\"@; \
         foreach($vk in 0x11,0x12,0x10,0x1B){ [K]::keybd_event($vk,0,0,[IntPtr]::Zero); Start-Sleep -Milliseconds 40 }; \
         foreach($vk in 0x1B,0x10,0x12,0x11){ [K]::keybd_event($vk,0,2,[IntPtr]::Zero); Start-Sleep -Milliseconds 40 }",
    );
    tokio::time::timeout(Duration::from_secs(3), human::estop_notified())
        .await
        .expect("the chord must fire the emergency stop");
    assert!(human::halted(), "e-stop must latch");
    assert_eq!(
        human::last_human_input_ms(),
        before,
        "injected keys must not be mistaken for a human (or you touched the keyboard/mouse          during the run — re-run hands-off)"
    );
    human::rearm();
    assert!(!human::halted());
}
