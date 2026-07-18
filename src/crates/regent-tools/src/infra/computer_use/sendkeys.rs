//! Keyboard translation for the PowerShell backend: literal text and key combos
//! into `System.Windows.Forms.SendKeys` strings.

/// Escape literal text for SendKeys (its metacharacters `{}()+^%~[]` must be
/// wrapped in braces to be sent literally).
pub(super) fn escape_sendkeys(text: &str) -> String {
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
/// Win/Cmd combos are an ERROR: SendKeys has no Win modifier, and silently
/// dropping it would type the bare key into the focused window — a wrong
/// action, worse than a refused one.
pub(super) fn combo_to_sendkeys(combo: &str) -> Result<String, String> {
    let mut prefix = String::new();
    let mut key = String::new();
    for part in combo.split('+') {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => prefix.push('^'),
            "alt" | "option" => prefix.push('%'),
            "shift" => prefix.push('+'),
            modifier @ ("win" | "super" | "meta" | "cmd") => {
                return Err(format!(
                    "the PowerShell backend cannot send the '{modifier}' modifier (SendKeys has \
                     no Win key) — use another route (e.g. the terminal tool, or a ctrl/alt \
                     shortcut)"
                ));
            }
            other => key = named_key(other),
        }
    }
    Ok(format!("{prefix}{key}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sendkeys_escaping_and_combos() {
        assert_eq!(escape_sendkeys("a+b(c)"), "a{+}b{(}c{)}");
        assert_eq!(combo_to_sendkeys("ctrl+s").unwrap(), "^s");
        assert_eq!(combo_to_sendkeys("alt+f4").unwrap(), "%{F4}");
        assert_eq!(combo_to_sendkeys("enter").unwrap(), "{ENTER}");
        assert_eq!(combo_to_sendkeys("ctrl+shift+t").unwrap(), "^+t");
        // Win/Cmd combos error loudly instead of typing the bare key into
        // whatever window happens to be focused.
        let e = combo_to_sendkeys("win+r").unwrap_err();
        assert!(e.contains("'win' modifier"), "{e}");
        assert!(combo_to_sendkeys("cmd+w").is_err());
    }
}
