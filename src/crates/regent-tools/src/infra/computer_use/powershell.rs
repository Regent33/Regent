//! Native-Windows [`ComputerBackend`]: screen capture via `System.Drawing`,
//! input via user32 P/Invoke — generated PowerShell run through a temp script
//! (same mechanism as `control_app`; no new native deps). Errors on non-Windows.

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
                    "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
                     $bounds=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
                     $bmp=New-Object System.Drawing.Bitmap($bounds.Width,$bounds.Height); \
                     $g=[System.Drawing.Graphics]::FromImage($bmp); \
                     $g.CopyFromScreen($bounds.Location,[System.Drawing.Point]::Empty,$bounds.Size); \
                     $bmp.Save('{p}',[System.Drawing.Imaging.ImageFormat]::Png); \
                     Write-Output (\"{{0}}x{{1}}\" -f $bounds.Width,$bounds.Height)"
                );
                let dims = run_ps(&script).await?;
                Ok(ActOutput {
                    note: format!("captured {}", dims.trim()),
                    image_path: Some(path.display().to_string()),
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
                let sk = combo_to_sendkeys(combo).replace('\'', "''");
                let script = format!(
                    "Add-Type -AssemblyName System.Windows.Forms; \
                     [System.Windows.Forms.SendKeys]::SendWait('{sk}')"
                );
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

/// user32 P/Invoke shim for mouse input, embedded once per click script.
const USER32: &str = "Add-Type @\"\nusing System;using System.Runtime.InteropServices;\nnamespace Regent { public class Native { [DllImport(\"user32.dll\")] public static extern bool SetCursorPos(int X,int Y); [DllImport(\"user32.dll\")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,IntPtr e); } }\n\"@";

/// Escape literal text for SendKeys (its metacharacters `{}()+^%~[]` must be
/// wrapped in braces to be sent literally).
fn escape_sendkeys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '{' | '}' | '(' | ')' | '+' | '^' | '%' | '~' | '[' | ']' => {
                out.push('{');
                out.push(c);
                out.push('}');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Translate a combo like `ctrl+s` / `alt+f4` / `enter` into a SendKeys string
/// (`^s`, `%{F4}`, `{ENTER}`). Unknown single tokens pass through escaped.
fn combo_to_sendkeys(combo: &str) -> String {
    let mut prefix = String::new();
    let mut key = String::new();
    for part in combo.split('+') {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => prefix.push('^'),
            "alt" | "option" => prefix.push('%'),
            "shift" => prefix.push('+'),
            "win" | "super" | "meta" | "cmd" => {} // SendKeys has no Win modifier
            other => key = named_key(other),
        }
    }
    format!("{prefix}{key}")
}

/// Map a key name to its SendKeys token (braced where required).
fn named_key(k: &str) -> String {
    match k {
        "enter" | "return" => "{ENTER}".into(),
        "tab" => "{TAB}".into(),
        "esc" | "escape" => "{ESC}".into(),
        "backspace" | "bksp" => "{BACKSPACE}".into(),
        "delete" | "del" => "{DELETE}".into(),
        "up" => "{UP}".into(),
        "down" => "{DOWN}".into(),
        "left" => "{LEFT}".into(),
        "right" => "{RIGHT}".into(),
        "home" => "{HOME}".into(),
        "end" => "{END}".into(),
        "pageup" => "{PGUP}".into(),
        "pagedown" => "{PGDN}".into(),
        "space" => " ".into(),
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            format!("{{{}}}", f.to_uppercase())
        }
        single if single.chars().count() == 1 => escape_sendkeys(single),
        other => escape_sendkeys(other),
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
        f.write_all(script.as_bytes())
            .await
            .map_err(|e| tool_err(e.to_string()))?;
        f.flush().await.map_err(|e| tool_err(e.to_string()))?;
    }
    let result = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&path)
        .output()
        .await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sendkeys_escaping_and_combos() {
        assert_eq!(escape_sendkeys("a+b(c)"), "a{+}b{(}c{)}");
        assert_eq!(combo_to_sendkeys("ctrl+s"), "^s");
        assert_eq!(combo_to_sendkeys("alt+f4"), "%{F4}");
        assert_eq!(combo_to_sendkeys("enter"), "{ENTER}");
        assert_eq!(combo_to_sendkeys("ctrl+shift+t"), "^+t");
    }
}
