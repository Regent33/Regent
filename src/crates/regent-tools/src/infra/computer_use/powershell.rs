//! Native-Windows [`ComputerBackend`]: screen capture via `System.Drawing`,
//! input via user32 P/Invoke — generated PowerShell run through a temp script
//! (same mechanism as `control_app`; no new native deps). Errors on non-Windows.
//! Script text lives in `ps_scripts`, keyboard translation in `sendkeys`.

use super::ps_scripts::{
    TabOp, USER32, close_window_script, focus_window_script, guard_prelude, keybd_event_script,
    tabs_script, type_script,
};
use super::sendkeys::{combo_to_sendkeys, keybd_combo};
use super::{ActOutput, Action, ComputerBackend, human};
use async_trait::async_trait;
use regent_kernel::RegentError;
use std::sync::Mutex;

/// Remembers the window the last `focus_window` put in front: every later
/// click/type/key insists that window is still the foreground one.
#[derive(Default)]
pub struct PowerShellBackend {
    pinned: Mutex<Option<i64>>,
}

impl PowerShellBackend {
    fn pinned(&self) -> Option<i64> {
        *self.pinned.lock().unwrap_or_else(|e| e.into_inner())
    }
    fn set_pinned(&self, hwnd: Option<i64>) {
        *self.pinned.lock().unwrap_or_else(|e| e.into_inner()) = hwnd;
    }
}

#[async_trait]
impl ComputerBackend for PowerShellBackend {
    async fn act(&self, action: &Action) -> Result<ActOutput, RegentError> {
        if !cfg!(windows) {
            return Err(tool_err(
                "PowerShell backend is Windows-only; configure a CUA backend elsewhere".into(),
            ));
        }
        if !action.is_mutating() {
            return self.run(action).await;
        }
        // The human wins: a real key or button press while the action runs
        // drops it (the child dies with the future) and reports a pause.
        let started = human::now_ms();
        tokio::select! {
            biased;
            () = human::wait_for_human_input(started) => Err(tool_err(
                "paused: the user is using the keyboard or mouse; wait for them to finish, \
                 re-check the screen, and do not retry until they say so"
                    .into(),
            )),
            result = self.run(action) => result,
        }
    }
}

impl PowerShellBackend {
    async fn run(&self, action: &Action) -> Result<ActOutput, RegentError> {
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
                let note = run_ps(&focus_window_script(*window_id)).await?;
                self.set_pinned(Some(*window_id));
                Ok(ActOutput {
                    note,
                    image_path: None,
                })
            }
            Action::CloseWindow { window_id } => {
                let note = run_ps(&close_window_script(*window_id)).await?;
                if self.pinned() == Some(*window_id) {
                    self.set_pinned(None);
                }
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
                    "{}; {USER32}; [Regent.Native]::SetCursorPos({x},{y}); \
                     [Regent.Native]::mouse_event(0x02,0,0,0,[System.IntPtr]::Zero); \
                     [Regent.Native]::mouse_event(0x04,0,0,0,[System.IntPtr]::Zero)",
                    guard_prelude(self.pinned())
                );
                run_ps(&script).await?;
                Ok(ActOutput {
                    note: format!("clicked ({x},{y})"),
                    image_path: None,
                })
            }
            Action::Type { text } => {
                let note = run_ps(&type_script(text, self.pinned())).await?;
                Ok(ActOutput {
                    note: note.trim().to_owned(),
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
                run_ps(&format!("{}; {script}", guard_prelude(self.pinned()))).await?;
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

pub(super) async fn run_ps(script: &str) -> Result<String, RegentError> {
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let path =
        std::env::temp_dir().join(format!("regent-cu-{}.ps1", uuid::Uuid::new_v4().simple()));
    // The script holds the text being typed; remove it whether this future
    // completes or is dropped by a stop (an `.await` after the child cannot
    // run on that path).
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
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
    // A stopped turn drops this future; the child MUST die with it. Without
    // this, a cancelled `type` kept sending a 3.6k-char document into every
    // window the user switched to — including Regent's own chat box, which
    // submitted the fragments as new user messages.
    cmd.kill_on_drop(true);
    // CREATE_NO_WINDOW: under a hidden deacon each action would otherwise pop
    // a console window that also STEALS FOCUS from the target right before
    // SendKeys fires, breaking the very keystroke being sent.
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    match cmd.output().await {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => Err(tool_err(format!(
            "powershell exited {}: {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr)
        ))),
        Err(e) => Err(tool_err(format!("powershell failed to run: {e}"))),
    }
}
