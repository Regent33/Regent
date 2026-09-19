//! Embedded PowerShell snippets for the native-Windows backend: the user32
//! P/Invoke shims and the window/tab automation scripts. Kept apart from the
//! backend dispatch so the (verbose) script text doesn't crowd the run logic.

use super::sendkeys::escape_sendkeys;

/// user32 P/Invoke shim for mouse input + DPI awareness, embedded per script.
/// `SetProcessDPIAware` first: without it a scaled display (125%/150% — most
/// Windows laptops) captures logical-size screenshots while clicks land in
/// virtualized coordinates, so the model aims at what it saw and misses.
pub(super) const USER32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Native { [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int X,int Y); [DllImport(\"user32.dll\")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,IntPtr e); [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware(); } }\n\"@\n[Regent.Native]::SetProcessDPIAware() | Out-Null";

const WINDOW32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class WindowNative { [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr h,int n); [DllImport(\"user32.dll\")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l); [DllImport(\"user32.dll\")] public static extern bool IsWindow(IntPtr h); } }\n\"@";

/// Close a top-level window, handling the browser "Close all tabs?" confirm.
///
/// `WM_CLOSE` on a browser window with several tabs pops a "Close all tabs?"
/// dialog and leaves the window open — the old close reported success anyway.
/// Now: post the close, and if the window survives (the dialog is up), accept
/// it — invoke a "Close all" button via UI Automation (no focus needed), else
/// focus the window and press Enter (its default button) — then VERIFY the
/// window handle is really gone before claiming success.
pub(super) fn close_window_script(window_id: i64) -> String {
    format!(
        "{WINDOW32}; Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes; \
         $handle=[IntPtr]{window_id}; \
         $process=Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -eq $handle }} | Select-Object -First 1; \
         if($null -eq $process){{ throw 'window_id is stale or not a visible top-level window; call list_windows again' }}; \
         $title=$process.MainWindowTitle; \
         [Regent.WindowNative]::PostMessage($handle,0x0010,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null; \
         Start-Sleep -Milliseconds 400; \
         if([Regent.WindowNative]::IsWindow($handle)){{ \
           $root=[System.Windows.Automation.AutomationElement]::FromHandle($handle); \
           $btnCond=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button); \
           $confirm=@($root.FindAll([System.Windows.Automation.TreeScope]::Descendants,$btnCond)) | Where-Object {{ $_.Current.Name -match '(?i)close all' }} | Select-Object -First 1; \
           $done=$false; \
           if($null -ne $confirm){{ try {{ $confirm.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke(); $done=$true }} catch {{}} }}; \
           if(-not $done){{ [Regent.WindowNative]::ShowWindowAsync($handle,9) | Out-Null; \
             [Regent.WindowNative]::SetForegroundWindow($handle) | Out-Null; \
             Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($process.Id) | Out-Null; Start-Sleep -Milliseconds 150; \
             Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('{{ENTER}}') }}; \
           Start-Sleep -Milliseconds 400 \
         }}; \
         if([Regent.WindowNative]::IsWindow($handle)){{ throw 'the window did not close; a confirm dialog may still be open' }} \
         else {{ Write-Output (\"closed window: {{0}}\" -f $title) }}"
    )
}

/// `GetForegroundWindow` / `GetWindowThreadProcessId` shim for the guard
/// prelude and the between-chunk typing check.
const FG32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Fg { [DllImport(\"user32.dll\")] public static extern IntPtr GetForegroundWindow(); [DllImport(\"user32.dll\")] public static extern uint GetWindowThreadProcessId(IntPtr h, out int pid); } }\n\"@";

/// The guard every mutating action runs first — the user outranks the agent:
///   * the foreground window must not belong to Regent itself (`regent-desktop`
///     in dev, `Regent` installed): an agent never needs to act on its own UI,
///     and this is exactly how the runaway-typing incident fed itself;
///   * with a pinned target (the last `focus_window`), the foreground must
///     belong to that window's PROCESS — the user switching apps changes the
///     process; the app's own dialog (a Save box, a confirm) does not, and
///     `list_windows` cannot address a dialog to re-focus it.
///
/// Leaves `$target` = the foreground window for the caller.
pub(super) fn guard_prelude(pinned: Option<i64>) -> String {
    let pin_check = match pinned {
        Some(hwnd) => format!(
            "$pinPid=0; [Regent.Fg]::GetWindowThreadProcessId([IntPtr]{hwnd},[ref]$pinPid) | Out-Null; \
             if($pinPid -eq 0 -or $fgPid -ne $pinPid){{ throw 'the target window is not in the foreground (the user switched away); call focus_window again, or wait for them' }}; "
        ),
        None => String::new(),
    };
    format!(
        "{FG32}; $fg=[Regent.Fg]::GetForegroundWindow(); $fgPid=0; \
         [Regent.Fg]::GetWindowThreadProcessId($fg,[ref]$fgPid) | Out-Null; \
         $fgName=(Get-Process -Id $fgPid -ErrorAction SilentlyContinue).ProcessName; \
         if($fgName -match '^regent'){{ throw (\"refusing: Regent's own window ({{0}}) is in the foreground; the agent must not act on its own app\" -f $fgName) }}; \
         {pin_check}$target=$fg"
    )
}

/// Characters per `SendWait` call. Typing is chunked so that (a) a stopped
/// turn, which kills the PowerShell child, leaves at most one chunk in the
/// input queue, and (b) the foreground check runs between chunks: the moment
/// another window comes forward — the user took the keyboard — typing stops
/// instead of following them from app to app. One 3.6k-char `SendWait` did
/// exactly that through a stop, into Google Docs, then Brave's other tabs,
/// then Regent's own chat box.
/// ponytail: 40 keeps a leak under a second; per-chunk cost is one P/Invoke.
pub(super) const TYPE_CHUNK: usize = 40;

/// Type `text` into the target window in [`TYPE_CHUNK`]-sized pieces, after
/// the guard prelude, aborting (with the count already typed) if the
/// foreground changes. Prints `typed N characters` on success.
pub(super) fn type_script(text: &str, pinned: Option<i64>) -> String {
    let chars: Vec<char> = text.chars().collect();
    // Two parallel arrays, not an array of pairs: `@(@('a',1))` flattens to
    // `@('a',1)` in PowerShell, so a single-chunk text would iterate its parts.
    let (texts, lens): (Vec<String>, Vec<String>) = chars
        .chunks(TYPE_CHUNK)
        .map(|chunk| {
            let raw: String = chunk.iter().collect();
            (
                format!("'{}'", escape_sendkeys(&raw).replace('\'', "''")),
                chunk.len().to_string(),
            )
        })
        .unzip();
    format!(
        "Add-Type -AssemblyName System.Windows.Forms; {}; \
         $texts=@({}); $lens=@({}); $sent=0; \
         for($i=0; $i -lt $texts.Count; $i++){{ \
           if([Regent.Fg]::GetForegroundWindow() -ne $target){{ \
             throw (\"typing stopped after {{0}} of {} characters: another window took the foreground (the user has the keyboard); do not retry until they say so\" -f $sent) }}; \
           [System.Windows.Forms.SendKeys]::SendWait($texts[$i]); $sent+=$lens[$i] }}; \
         Write-Output (\"typed {{0}} characters\" -f $sent)",
        guard_prelude(pinned),
        texts.join(","),
        lens.join(","),
        chars.len()
    )
}

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

/// `AttachThreadInput` shim: sharing the foreground thread's input queue is
/// what lets a background process satisfy the foreground lock.
const ATTACH32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Attach { [DllImport(\"user32.dll\")] public static extern bool AttachThreadInput(uint a,uint b,bool attach); [DllImport(\"kernel32.dll\")] public static extern uint GetCurrentThreadId(); } }\n\"@";

/// Bring `window_id` to the front and VERIFY it (by process) — `SetForegroundWindow`
/// lies: it returns true without focusing, and returns false under the
/// foreground lock (this process is not the one the user last used — the case
/// whenever the deacon was not launched by the app in front). Attaching to the
/// foreground thread's input queue for the call satisfies that lock without
/// injecting input (an ALT tap does too, but it toggles the target's menu
/// accelerators and ate the first ten typed characters in Notepad).
pub(super) fn focus_window_script(window_id: i64) -> String {
    window_script(
        window_id,
        &format!(
            "{FG32}; {ATTACH32}; [Regent.WindowNative]::ShowWindowAsync($handle,9) | Out-Null; \
             $d=0; $fgThread=[Regent.Fg]::GetWindowThreadProcessId([Regent.Fg]::GetForegroundWindow(),[ref]$d); \
             $me=[Regent.Attach]::GetCurrentThreadId(); \
             $attached=($fgThread -ne 0 -and $fgThread -ne $me -and [Regent.Attach]::AttachThreadInput($me,$fgThread,$true)); \
             [Regent.WindowNative]::SetForegroundWindow($handle) | Out-Null; \
             if($attached){{ [Regent.Attach]::AttachThreadInput($me,$fgThread,$false) | Out-Null }}; \
             Start-Sleep -Milliseconds 150; \
             $fgPid=0; [Regent.Fg]::GetWindowThreadProcessId([Regent.Fg]::GetForegroundWindow(),[ref]$fgPid) | Out-Null; \
             if($fgPid -ne $process.Id){{ throw 'Windows refused to focus the requested window' }}; \
             Write-Output (\"focused: {{0}}\" -f $process.MainWindowTitle)"
        ),
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
#[path = "tests/ps_scripts.rs"]
mod tests;
