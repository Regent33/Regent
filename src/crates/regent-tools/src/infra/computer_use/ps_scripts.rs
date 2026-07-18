//! Embedded PowerShell snippets for the native-Windows backend: the user32
//! P/Invoke shims and the window/tab automation scripts. Kept apart from the
//! backend dispatch so the (verbose) script text doesn't crowd the run logic.

/// user32 P/Invoke shim for mouse input + DPI awareness, embedded per script.
/// `SetProcessDPIAware` first: without it a scaled display (125%/150% — most
/// Windows laptops) captures logical-size screenshots while clicks land in
/// virtualized coordinates, so the model aims at what it saw and misses.
pub(super) const USER32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Native { [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int X,int Y); [DllImport(\"user32.dll\")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,IntPtr e); [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware(); } }\n\"@\n[Regent.Native]::SetProcessDPIAware() | Out-Null";

const WINDOW32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class WindowNative { [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport(\"user32.dll\")] public static extern bool ShowWindowAsync(IntPtr h,int n); [DllImport(\"user32.dll\")] public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l); } }\n\"@";

pub(super) fn window_script(window_id: i64, action: &str) -> String {
    format!(
        "{WINDOW32}; $handle=[IntPtr]{window_id}; \
         $process=Get-Process -ErrorAction SilentlyContinue | Where-Object {{ $_.MainWindowHandle -eq $handle }} | Select-Object -First 1; \
         if($null -eq $process){{ throw 'window_id is stale or not a visible top-level window; call list_windows again' }}; \
         {action}"
    )
}

pub(super) fn tabs_script(window_id: i64, close_target: Option<&str>) -> String {
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
    let Some(target) = close_target else {
        return format!(
            "{prefix}; ConvertTo-Json -InputObject @($tabs | ForEach-Object {{ $_.Current.Name }}) -Compress"
        );
    };
    let target = target.replace('\'', "''");
    format!(
        "{prefix}; $target='{target}'; \
         $matches=@($tabs | Where-Object {{ $_.Current.Name -eq $target }}); \
         if($matches.Count -eq 0){{ $matches=@($tabs | Where-Object {{ $_.Current.Name.IndexOf($target,[StringComparison]::OrdinalIgnoreCase) -ge 0 }}) }}; \
         if($matches.Count -eq 0){{ throw 'tab title was not found; call list_tabs again' }}; \
         if($matches.Count -ne 1){{ throw (\"tab title is ambiguous: {{0}}\" -f (($matches | ForEach-Object {{ $_.Current.Name }}) -join ' | ')) }}; \
         $name=$matches[0].Current.Name; \
         $pattern=$matches[0].GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern); \
         $pattern.Select(); [Regent.WindowNative]::ShowWindowAsync($handle,9) | Out-Null; \
         [Regent.WindowNative]::SetForegroundWindow($handle) | Out-Null; \
         Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate($process.Id) | Out-Null; Start-Sleep -Milliseconds 150; \
         Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^w'); \
         Write-Output (\"closed tab: {{0}}\" -f $name)"
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

        let tabs = tabs_script(42, Some("Rainer's docs"));
        assert!(tabs.contains("$target='Rainer''s docs'"));
        assert!(tabs.contains("SelectionItemPattern"));
        assert!(tabs.contains("AppActivate($process.Id)"));
    }
}
