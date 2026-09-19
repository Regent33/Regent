//! Script-generation checks for the native-Windows backend.

use super::*;

#[test]
fn target_scripts_use_exact_window_ids_and_escape_tab_titles() {
    let focus = window_script(42, "Write-Output 'ok'");
    assert!(focus.contains("$handle=[IntPtr]42"));
    assert!(focus.contains("window_id is stale"));

    let tabs = tabs_script(42, TabOp::Close("Rainer's docs"));
    assert!(tabs.contains("$target='Rainer''s docs'"));
    assert!(tabs.contains("SelectionItemPattern"));
    assert!(tabs.contains("AppActivate($process.Id)"));
    // Close invokes the tab's close button, then verifies by re-scanning
    // for the name (a stale-element check gave false negatives) — never a
    // blind Ctrl+W that lies about success.
    assert!(tabs.contains("InvokePattern"));
    assert!(tabs.contains("Where-Object { $_.Current.Name -eq $name }"));

    // Select switches tabs and confirms the switch took.
    let select = tabs_script(7, TabOp::Select("docs"));
    assert!(select.contains("$target='docs'"));
    assert!(select.contains(".Select()"));
    assert!(select.contains("IsSelected"));

    // List is a plain read — no matching, no mutation.
    let list = tabs_script(7, TabOp::List);
    assert!(list.contains("ConvertTo-Json"));
    assert!(!list.contains("$target="));

    // close_window handles the browser "Close all tabs?" confirm: invoke
    // the "Close all" button (or Enter), then verify the HWND is gone.
    let close_win = close_window_script(6357108);
    assert!(close_win.contains("$handle=[IntPtr]6357108"));
    assert!(close_win.contains("close all"));
    assert!(close_win.contains("SendWait('{ENTER}')"));
    assert!(close_win.contains("IsWindow($handle)"));

    // Braces balance (a stray {{ from the format! split would break PS).
    for s in [&tabs, &select, &close_win] {
        assert_eq!(
            s.matches('{').count(),
            s.matches('}').count(),
            "unbalanced braces in tab script"
        );
        assert!(
            !s.contains("{{") && !s.contains("}}"),
            "double braces leaked into PS"
        );
    }
}

#[test]
fn type_script_chunks_text_and_stops_when_the_foreground_changes() {
    // 100 chars → 40 + 40 + 20; a metachar and a quote sit inside chunks
    // and must be escaped per chunk, never split across two.
    let text = format!("{}+{}'{}", "a".repeat(39), "b".repeat(39), "c".repeat(20));
    assert_eq!(text.chars().count(), 100);
    let s = type_script(&text, None);
    let expected = format!(
        "$texts=@('{}{{+}}','{}''','{}')",
        "a".repeat(39),
        "b".repeat(39),
        "c".repeat(20)
    );
    assert!(s.contains(&expected), "{s}");
    assert!(s.contains("$lens=@(40,40,20)"), "{s}");
    assert!(s.contains("of 100 characters"), "{s}");
    // The guard runs BEFORE every SendWait, against the window typing
    // started in — so a user switching apps mid-text stops it.
    let guard = s.find("GetForegroundWindow() -ne $target").expect("guard");
    let send = s.find("SendKeys]::SendWait($texts[$i])").expect("send");
    assert!(guard < send, "guard must precede the keystrokes");
    assert!(s.contains("the user has the keyboard"), "{s}");
    assert!(s.contains("typed {0} characters"), "{s}");
    assert_eq!(
        s.matches('{').count(),
        s.matches('}').count(),
        "unbalanced braces"
    );
    assert!(
        !s.contains("{{") && !s.contains("}}"),
        "double braces leaked into PS"
    );

    // One chunk and no chunks are still well-formed arrays.
    assert!(type_script("hi", None).contains("$texts=@('hi'); $lens=@(2)"));
    assert!(type_script("", None).contains("$texts=@(); $lens=@()"));
}

#[test]
fn guard_prelude_refuses_regents_own_window_and_pins_the_target() {
    // No pin: the foreground IS the target, but never Regent's own app.
    let free = guard_prelude(None);
    assert!(free.contains("$fgName -match '^regent'"), "{free}");
    assert!(free.ends_with("$target=$fg"), "{free}");
    assert!(!free.contains("$pinPid"), "{free}");
    // Pinned: the foreground must belong to that window's process (so the
    // app's own dialogs pass, another app does not).
    let pinned = guard_prelude(Some(524996));
    assert!(
        pinned.contains("GetWindowThreadProcessId([IntPtr]524996,[ref]$pinPid)"),
        "{pinned}"
    );
    assert!(pinned.contains("$fgPid -ne $pinPid"), "{pinned}");
    assert!(pinned.contains("the user switched away"), "{pinned}");
    assert!(pinned.ends_with("$target=$fg"), "{pinned}");
    // The prelude runs BEFORE the chunk loop in a typing script.
    let typed = type_script("abc", Some(7));
    let guard = typed.find("[IntPtr]7,[ref]$pinPid").expect("pinned target");
    let send = typed.find("SendWait($texts[$i])").expect("send");
    assert!(guard < send);
    for s in [&free, &pinned, &typed] {
        assert_eq!(s.matches('{').count(), s.matches('}').count(), "braces");
        assert!(
            !s.contains("{{") && !s.contains("}}"),
            "double braces leaked"
        );
    }
}

#[test]
fn keybd_event_presses_down_then_releases_in_reverse() {
    // win(0x5B=91) + shift(0x10=16) + s(0x53=83).
    let s = keybd_event_script(&[0x5B, 0x10], 0x53);
    assert!(s.contains("Regent.Kbd"), "shim missing");
    // flags: 0 = key-down, 2 = key-up.
    let at = |needle: &str| s.find(needle).unwrap_or_else(|| panic!("missing {needle}"));
    let down_win = at("::keybd_event(91,0,0,");
    let down_shift = at("::keybd_event(16,0,0,");
    let down_key = at("::keybd_event(83,0,0,");
    let up_key = at("::keybd_event(83,0,2,");
    let up_shift = at("::keybd_event(16,0,2,");
    let up_win = at("::keybd_event(91,0,2,");
    // Modifiers down, then key; key up before modifiers; modifiers released
    // in REVERSE order (shift before win).
    assert!(down_win < down_shift && down_shift < down_key, "down order");
    assert!(down_key < up_key, "key tapped");
    assert!(up_key < up_shift && up_shift < up_win, "reverse release");
    // A gap between events, or the OS drops the modifier hold. 6 events →
    // 5 gaps.
    assert_eq!(
        s.matches("Start-Sleep").count(),
        5,
        "one gap between events"
    );
}

#[test]
fn keybd_event_with_no_modifiers_just_taps_the_key() {
    // PrintScreen (0x2C = 44), no modifiers: one down, one up, nothing else.
    // Count `::keybd_event(` (invocations) — the shim's P/Invoke
    // declaration also contains the bare word `keybd_event(`.
    let s = keybd_event_script(&[], 0x2C);
    assert_eq!(s.matches("::keybd_event(").count(), 2, "one down + one up");
    assert!(s.contains("::keybd_event(44,0,0,") && s.contains("::keybd_event(44,0,2,"));
    // 2 events → exactly 1 gap between the down and the up.
    assert_eq!(s.matches("Start-Sleep").count(), 1);
}
