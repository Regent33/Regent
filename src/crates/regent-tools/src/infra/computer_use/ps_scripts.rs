//! Embedded PowerShell snippets for the native-Windows backend: the user32
//! P/Invoke shims and the window/tab automation scripts. Kept apart from the
//! backend dispatch so the (verbose) script text doesn't crowd the run logic.

/// user32 P/Invoke shim for mouse input + DPI awareness, embedded per script.
/// `SetProcessDPIAware` first: without it a scaled display (125%/150% — most
/// Windows laptops) captures logical-size screenshots while clicks land in
/// virtualized coordinates, so the model aims at what it saw and misses.
pub(super) const USER32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Native { [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int X,int Y); [DllImport(\"user32.dll\")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,IntPtr e); [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware(); } }\n\"@\n[Regent.Native]::SetProcessDPIAware() | Out-Null";

const WINDOW32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class WindowNative { [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr h,int n); [DllImport(\"user32.dll\")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l); } }\n\"@";

/// `keybd_event` shim for VK-code key injection — used for shortcuts SendKeys
/// can't express (the Windows key), pressing modifiers then the key and
/// releasing in reverse.
pub(super) const KEYBD32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Kbd { [DllImport(\"user32.dll\")] public static extern void keybd_event(byte vk,byte scan,uint flags,IntPtr extra); } }\n\"@";

/// Gap between injected key events. A back-to-back modifier combo (win+r) is
/// dropped by the OS — the modifier's hold doesn't register before the key —
/// verified empirically; ~40ms fixes it and is imperceptible.
/// ponytail: bump if a slow machine still drops combos.
const KEY_GAP_MS: u32 = 40;

/// Build a `keybd_event` down/up sequence for `modifiers`+`key` (VK codes):
/// press modifiers, tap the key, release modifiers in reverse, with a short
/// sleep between each event. `flags` 0 = key-down, 2 = key-up
/// (KEYEVENTF_KEYUP).
pub(super) fn keybd_event_script(modifiers: &[u8], key: u8) -> String {
    let ev = |vk: u8, up: bool| {
        format!(
            "[Regent.Kbd]::keybd_event({vk},0,{},[System.IntPtr]::Zero)",
            if up { 2 } else { 0 }
        )
    };
    let mut events: Vec<String> = modifiers.iter().map(|&m| ev(m, false)).collect();
    events.push(ev(key, false));
    events.push(ev(key, true));
    events.extend(modifiers.iter().rev().map(|&m| ev(m, true)));
    format!(
        "{KEYBD32}; {}",
        events.join(&format!("; Start-Sleep -Milliseconds {KEY_GAP_MS}; "))
    )
}

pub(super) fn window_script(window_id: i64, action: &str) -> String {
    format!(
        "{WINDOW32}; $handle=[IntPtr]{window_id}; \
         $process=Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -eq $handle }} | Select-Object -First 1; \
         if($null -eq $process){{ throw 'window_id is stale or not a visible top-level window; call list_windows again' }}; \
         {action}"
    )
}

/// What `tabs_script` should do with a window's tabs.
pub(super) enum TabOp<'a> {
    /// JSON array of tab titles.
    List,
    /// Make the named tab the active one (choose which tab).
    Select(&'a str),
    /// Close the named tab.
    Close(&'a str),
}

pub(super) fn tabs_script(window_id: i64, op: TabOp<'_>) -> String {
    // UIAutomation over ALL descendant TabItems. NOTE: this includes in-page
    // web "tabs" (a site's own tab strip), not only the browser's — the
    // browser tabs are the ones that carry a close Button, which is why close
    // uses that, and why matching is by (usually unique) title.
    let prefix = format!(
        "{WINDOW32}; Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes; \
         $handle=[IntPtr]{window_id}; \
         $process=Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -eq $handle }} | Select-Object -First 1; \
         if($null -eq $process){{ throw 'window_id is stale or not a visible top-level window; call list_windows again' }}; \
         $root=[System.Windows.Automation.AutomationElement]::FromHandle($handle); \
         $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::TabItem); \
         $items=$root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition); \
         $tabs=@($items | ForEach-Object {{ $_ }} | Where-Object {{ $_.Current.Name }})"
    );
    let (target, tail) = match op {
        TabOp::List => {
            return format!(
                "{prefix}; ConvertTo-Json -InputObject @($tabs | ForEach-Object {{ $_.Current.Name }}) -Compress"
            );
        }
        // Selecting a tab is a UIA pattern call — it needs no window focus, so
        // the Windows foreground lock (which made SendKeys unreliable) can't
        // stop it. Bring the window forward too so the user sees the switch.
        TabOp::Select(target) => (
            target,
            "$tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select(); \
             [Regent.WindowNative]::ShowWindowAsync($handle,9) | Out-Null; \
             [Regent.WindowNative]::SetForegroundWindow($handle) | Out-Null; \
             Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($process.Id) | Out-Null; \
             Start-Sleep -Milliseconds 120; \
             if($tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected){ Write-Output (\"selected tab: {0}\" -f $name) } \
             else { throw 'the browser did not switch to that tab; call list_tabs again' }",
        ),
        // The old close blind-fired Ctrl+W after trying to focus the window and
        // reported success unconditionally — a focus-lock failure closed
        // nothing (or the wrong tab) yet still said "closed". Now: activate the
        // tab (so it's the one Ctrl+W acts on and its close button is present),
        // then close it and VERIFY. Primary path invokes ONLY a button named
        // "close" — never a random first button, so the mute/audio button on a
        // playing tab is never hit. If no such button (odd browser/locale),
        // fall back to focusing the window and pressing Ctrl+W (the key path).
        TabOp::Close(target) => (
            target,
            "try { $tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select() } catch {}; \
             Start-Sleep -Milliseconds 150; \
             $btnCond=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button); \
             $close=@($tab.FindAll([System.Windows.Automation.TreeScope]::Descendants,$btnCond)) | Where-Object { $_.Current.Name -match '(?i)close' } | Select-Object -First 1; \
             $closed=$false; \
             if($null -ne $close){ try { $close.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); $closed=$true } catch {} }; \
             if(-not $closed){ [Regent.WindowNative]::ShowWindowAsync($handle,9) | Out-Null; \
               [Regent.WindowNative]::SetForegroundWindow($handle) | Out-Null; \
               Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($process.Id) | Out-Null; Start-Sleep -Milliseconds 150; \
               Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^w') }; \
             Start-Sleep -Milliseconds 400; \
             $after=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$condition) | Where-Object { $_.Current.Name -eq $name }); \
             if($after.Count -eq 0){ Write-Output (\"closed tab: {0}\" -f $name) } \
             else { throw 'the tab did not close; try again, or the browser may have a save/confirm prompt open' }",
        ),
    };
    let target = target.replace('\'', "''");
    // Shared match: exact title, then case-insensitive substring; reject
    // not-found and ambiguous so an action never hits the wrong tab.
    format!(
        "{prefix}; $target='{target}'; \
         $matches=@($tabs | Where-Object {{ $_.Current.Name -eq $target }}); \
         if($matches.Count -eq 0){{ $matches=@($tabs | Where-Object {{ $_.Current.Name.IndexOf($target,[StringComparison]::OrdinalIgnoreCase) -ge 0 }}) }}; \
         if($matches.Count -eq 0){{ throw 'tab title was not found; call list_tabs again' }}; \
         if($matches.Count -ne 1){{ throw (\"tab title is ambiguous: {{0}}\" -f (($matches | ForEach-Object {{ $_.Current.Name }}) -join ' | ')) }}; \
         $tab=$matches[0]; $name=$tab.Current.Name; {tail}"
    )
}

#[cfg(test)]
mod tests {
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

        // Braces balance (a stray {{ from the format! split would break PS).
        for s in [&tabs, &select] {
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
}
