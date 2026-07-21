//! Native-Windows [`ComputerBackend`]: screen capture via `System.Drawing`,
//! input via user32 P/Invoke — generated PowerShell run through a temp script
//! (same mechanism as `control_app`; no new native deps). Errors on non-Windows.
//! Script text lives in `ps_scripts`, keyboard translation in `sendkeys`.

use super::ps_scripts::{
    TabOp, USER32, close_window_script, keybd_event_script, tabs_script, window_script,
};
use super::sendkeys::{combo_to_sendkeys, escape_sendkeys, keybd_combo};
use super::{ActOutput, Action, ComputerBackend};
use async_trait::async_trait;
use regent_kernel::RegentError;

pub struct PowerShellBackend;

#[async_trait]
impl ComputerBackend for PowerShellBackend {
    async fn act(&self, action: &Action) -> Result<ActOutput, RegentError> {
        if !cfg!(windows) {
            return Err(tool_err(
                "PowerShell backend is Windows-only; configure a CUA backend elsewhere".into(),
            ));
        }
        match action {
            Action::Screenshot => {
                let path = std::env::temp_dir()
                    .join(format!("regent-shot-{}.png", uuid::Uuid::new_v4().simple()));
                let p = path.display().to_string().replace('\'', "''");
                let script = format!(
                    "{USER32}; Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
                     $bounds=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
                     $bmp=New-Object System.Drawing.Bitmap($bounds.Width,$bounds.Height); \
                     $g=[System.Drawing.Graphics]::FromImage($bmp); \
                     $g.CopyFromScreen($bounds.Location,[System.Drawing.Point]::Empty,$bounds.Size); \
                     $bmp.Save('{p}',[System.Drawing.Imaging.ImageFormat]::Png); \
                     Write-Output (\"{{0}}x{{1}}\" -f $bounds.Width,$bounds.Height)"
                );
                let dims = run_ps(&script).await?;
                Ok(ActOutput {
                    // ponytail: primary screen only — clicks CAN land on other
                    // monitors (virtual-desktop coords), so say what was seen.
                    note: format!("captured {} (primary screen only)", dims.trim()),
                    image_path: Some(path.display().to_string()),
                })
            }
            Action::ListWindows => {
                let note = run_ps(
                    "$rows = Get-Process -ErrorAction SilentlyContinue | \
                     Where-Object { $_.MainWindowHandle -ne 0 -and $_.MainWindowTitle } | \
                     ForEach-Object { [pscustomobject]@{ window_id = $_.MainWindowHandle.ToInt64(); \
                     process_id = $_.Id; process = $_.ProcessName; title = $_.MainWindowTitle } }; \
                     ConvertTo-Json -InputObject @($rows) -Compress",
                )
                .await?;
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::FocusWindow { window_id } => {
                let note = run_ps(&window_script(
                    *window_id,
                    "[Regent.WindowNative]::ShowWindowAsync($handle,9) | Out-Null; \
                     if(-not [Regent.WindowNative]::SetForegroundWindow($handle)){ throw 'Windows refused to focus the requested window' }; \
                     Write-Output (\"focused: {0}\" -f $process.MainWindowTitle)",
                ))
                .await?;
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::CloseWindow { window_id } => {
                let note = run_ps(&close_window_script(*window_id)).await?;
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::ListTabs { window_id } => {
                let note = run_ps(&tabs_script(*window_id, TabOp::List)).await?;
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::SelectTab { window_id, target } => {
                let note = run_ps(&tabs_script(*window_id, TabOp::Select(target))).await?;
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::CloseTab { window_id, target } => {
                let note = run_ps(&tabs_script(*window_id, TabOp::Close(target))).await?;
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::Click { x, y } => {
                let script = format!(
                    "{USER32}; [Regent.Native]::SetCursorPos({x},{y}); \
                     [Regent.Native]::mouse_event(0x02,0,0,0,[System.IntPtr]::Zero); \
                     [Regent.Native]::mouse_event(0x04,0,0,0,[System.IntPtr]::Zero)"
                );
                run_ps(&script).await?;
                Ok(ActOutput {
                    note: format!("clicked ({x},{y})"),
                    image_path: None,
                })
            }
            Action::Type { text } => {
                let escaped = escape_sendkeys(text).replace('\'', "''");
                let script = format!(
                    "Add-Type -AssemblyName System.Windows.Forms; \
                     [System.Windows.Forms.SendKeys]::SendWait('{escaped}')"
                );
                run_ps(&script).await?;
                Ok(ActOutput {
                    note: "typed text".into(),
                    image_path: None,
                })
            }
            Action::Key { combo } => {
                // Win-key shortcuts and media/browser keys can't go through
                // SendKeys, so route those through keybd_event VK codes;
                // everything else stays SendKeys.
                let script = match keybd_combo(combo) {
                    Some(vks) => {
                        let (modifiers, key) = vks.map_err(tool_err)?;
                        keybd_event_script(&modifiers, key)
                    }
                    None => {
                        let sk = combo_to_sendkeys(combo)
                            .map_err(tool_err)?
                            .replace('\'', "''");
                        format!(
                            "Add-Type -AssemblyName System.Windows.Forms; \
                             [System.Windows.Forms.SendKeys]::SendWait('{sk}')"
                        )
                    }
                };
                run_ps(&script).await?;
                Ok(ActOutput {
                    note: format!("pressed {combo}"),
                    image_path: None,
                })
            }
        }
    }
}

fn tool_err(message: String) -> RegentError {
    RegentError::Tool {
        tool: "computer_use".into(),
        message,
    }
}

async fn run_ps(script: &str) -> Result<String, RegentError> {
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let path =
        std::env::temp_dir().join(format!("regent-cu-{}.ps1", uuid::Uuid::new_v4().simple()));
    {
        let mut f = tokio::fs::File::create(&path)
            .await
            .map_err(|e| tool_err(e.to_string()))?;
        // UTF-8 BOM: Windows PowerShell 5.1 reads a BOM-less .ps1 as ANSI,
        // which mojibakes any non-ASCII text being typed (accents, CJK, …).
        f.write_all(b"\xEF\xBB\xBF")
            .await
            .map_err(|e| tool_err(e.to_string()))?;
        f.write_all(script.as_bytes())
            .await
            .map_err(|e| tool_err(e.to_string()))?;
        f.flush().await.map_err(|e| tool_err(e.to_string()))?;
    }
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&path);
    // CREATE_NO_WINDOW: under a hidden deacon each action would otherwise pop
    // a console window that also STEALS FOCUS from the target right before
    // SendKeys fires, breaking the very keystroke being sent.
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let result = cmd.output().await;
    let _ = tokio::fs::remove_file(&path).await;
    match result {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => Err(tool_err(format!(
            "powershell exited {}: {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr)
        ))),
        Err(e) => Err(tool_err(format!("powershell failed to run: {e}"))),
    }
}
