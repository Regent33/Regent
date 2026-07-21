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

/// VK codes for a Win-key combo — SendKeys has no Win modifier, so those combos
/// go through `keybd_event` instead (see the PowerShell backend). Returns
/// `None` when the combo has no Win/super/meta/cmd modifier (SendKeys handles
/// it); `Some(Err)` when a key in the combo has no VK mapping (better a loud
/// error than a wrong key). On success: (modifier VKs in press order, key VK).
pub(super) fn win_combo_vks(combo: &str) -> Option<Result<(Vec<u8>, u8), String>> {
    let parts: Vec<String> = combo.split('+').map(|p| p.trim().to_lowercase()).collect();
    if !parts
        .iter()
        .any(|p| matches!(p.as_str(), "win" | "super" | "meta" | "cmd"))
    {
        return None;
    }
    let mut modifiers = Vec::new();
    let mut key = None;
    for part in &parts {
        match part.as_str() {
            "win" | "super" | "meta" | "cmd" => modifiers.push(0x5B), // VK_LWIN
            "ctrl" | "control" => modifiers.push(0x11),               // VK_CONTROL
            "alt" | "option" => modifiers.push(0x12),                 // VK_MENU
            "shift" => modifiers.push(0x10),                          // VK_SHIFT
            other => match key_to_vk(other) {
                Some(vk) => key = Some(vk),
                None => {
                    return Some(Err(format!(
                        "can't send '{other}' as part of a Windows-key shortcut"
                    )));
                }
            },
        }
    }
    Some(match key {
        Some(vk) => Ok((modifiers, vk)),
        None => Err("a Windows-key shortcut needs a key, e.g. win+r".into()),
    })
}

/// Virtual-key code for a key name (the subset reachable in a Win combo).
fn key_to_vk(k: &str) -> Option<u8> {
    Some(match k {
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "esc" | "escape" => 0x1B,
        "space" | "spacebar" => 0x20,
        "backspace" | "bksp" | "bs" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "printscreen" | "prtsc" | "prtscr" => 0x2C,
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
            0x70 + (f[1..].parse::<u8>().unwrap() - 1) // VK_F1 = 0x70
        }
        s if s.chars().count() == 1 => {
            let c = s.chars().next().unwrap();
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase() as u8 // 'A'-'Z' / '0'-'9' == their VK
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

/// Map a key name to its SendKeys token (braced where required). Covers the
/// full SendKeys special-key vocabulary plus the aliases models actually emit
/// (`pgup`, `ins`, `prtsc`, …); the Windows/media keys SendKeys can't produce
/// are handled by the caller (Win errors loudly; a bare word falls through to
/// literal text rather than a wrong keypress).
fn named_key(k: &str) -> String {
    match k {
        "enter" | "return" => "{ENTER}".into(),
        "tab" => "{TAB}".into(),
        "esc" | "escape" => "{ESC}".into(),
        "backspace" | "bksp" | "bs" => "{BACKSPACE}".into(),
        "delete" | "del" => "{DELETE}".into(),
        "insert" | "ins" => "{INSERT}".into(),
        "up" => "{UP}".into(),
        "down" => "{DOWN}".into(),
        "left" => "{LEFT}".into(),
        "right" => "{RIGHT}".into(),
        "home" => "{HOME}".into(),
        "end" => "{END}".into(),
        "pageup" | "pgup" => "{PGUP}".into(),
        "pagedown" | "pgdn" => "{PGDN}".into(),
        "space" | "spacebar" => " ".into(),
        "capslock" => "{CAPSLOCK}".into(),
        "numlock" => "{NUMLOCK}".into(),
        "scrolllock" => "{SCROLLLOCK}".into(),
        "printscreen" | "prtsc" | "prtscr" => "{PRTSC}".into(),
        "break" => "{BREAK}".into(),
        "help" => "{HELP}".into(),
        // Numpad operators (the letters/digits are just literal chars).
        "add" | "plus" => "{ADD}".into(),
        "subtract" | "minus" => "{SUBTRACT}".into(),
        "multiply" => "{MULTIPLY}".into(),
        "divide" => "{DIVIDE}".into(),
        // SendKeys only understands F1–F16 (it errors on {F17}+); higher F-keys
        // must go through the keybd_event path, so anything else falls through
        // to literal text rather than a SendKeys error.
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok_and(|n| (1..=16).contains(&n)) => {
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

    #[test]
    fn extended_named_keys_and_aliases() {
        // The keys the old map was missing, with the aliases models emit.
        assert_eq!(combo_to_sendkeys("insert").unwrap(), "{INSERT}");
        assert_eq!(combo_to_sendkeys("ins").unwrap(), "{INSERT}");
        assert_eq!(combo_to_sendkeys("pgup").unwrap(), "{PGUP}");
        assert_eq!(combo_to_sendkeys("prtsc").unwrap(), "{PRTSC}");
        assert_eq!(combo_to_sendkeys("capslock").unwrap(), "{CAPSLOCK}");
        assert_eq!(combo_to_sendkeys("ctrl+add").unwrap(), "^{ADD}");
        assert_eq!(combo_to_sendkeys("f12").unwrap(), "{F12}");
        // Editing shortcuts the model needs for input fields.
        assert_eq!(combo_to_sendkeys("ctrl+a").unwrap(), "^a");
        assert_eq!(combo_to_sendkeys("shift+end").unwrap(), "+{END}");
    }

    #[test]
    fn sendkeys_f_key_boundary_is_f16() {
        // SendKeys supports F1–F16; F17+ would make SendKeys throw, so it must
        // NOT be emitted as {F17} — it degrades to literal text instead.
        assert_eq!(combo_to_sendkeys("f16").unwrap(), "{F16}");
        assert_eq!(combo_to_sendkeys("f17").unwrap(), "f17");
        assert_eq!(combo_to_sendkeys("f99").unwrap(), "f99");
        assert_eq!(combo_to_sendkeys("f0").unwrap(), "f0");
    }

    #[test]
    fn win_combos_map_to_vk_codes_not_sendkeys() {
        // win+r → LWIN modifier + 'R'. Aliases all mean the same key.
        for alias in ["win+r", "super+r", "meta+r", "cmd+r"] {
            assert_eq!(
                win_combo_vks(alias).unwrap().unwrap(),
                (vec![0x5B], 0x52),
                "{alias}"
            );
        }
        // Press order preserved; win+shift+s is the Windows snip shortcut.
        assert_eq!(
            win_combo_vks("win+shift+s").unwrap().unwrap(),
            (vec![0x5B, 0x10], 0x53)
        );
        // Every modifier before the key: ctrl+alt+win+delete.
        assert_eq!(
            win_combo_vks("ctrl+alt+win+delete").unwrap().unwrap(),
            (vec![0x11, 0x12, 0x5B], 0x2E)
        );
        // Named keys and F-keys reach VK codes (F-keys go to F24 here, unlike
        // SendKeys' F16 ceiling).
        assert_eq!(
            win_combo_vks("win+left").unwrap().unwrap(),
            (vec![0x5B], 0x25)
        );
        assert_eq!(
            win_combo_vks("win+f4").unwrap().unwrap(),
            (vec![0x5B], 0x73)
        );

        // Non-Win combos are None — SendKeys owns those.
        assert!(win_combo_vks("ctrl+s").is_none());
        assert!(win_combo_vks("enter").is_none());

        // Loud errors, never a wrong key: no key, only modifiers, or an
        // unmappable key.
        assert!(win_combo_vks("win").unwrap().is_err());
        assert!(win_combo_vks("win+ctrl").unwrap().is_err());
        assert!(win_combo_vks("win+volumeup").unwrap().is_err());
        assert!(win_combo_vks("win+;").unwrap().is_err());
    }

    #[test]
    fn key_to_vk_covers_letters_digits_named_and_f_key_bounds() {
        assert_eq!(key_to_vk("a"), Some(0x41));
        assert_eq!(key_to_vk("z"), Some(0x5A));
        assert_eq!(key_to_vk("0"), Some(0x30));
        assert_eq!(key_to_vk("9"), Some(0x39));
        assert_eq!(key_to_vk("enter"), Some(0x0D));
        assert_eq!(key_to_vk("pgup"), Some(0x21));
        // VK has F1 (0x70) through F24 (0x87); F25 and F0 are out of range.
        assert_eq!(key_to_vk("f1"), Some(0x70));
        assert_eq!(key_to_vk("f24"), Some(0x87));
        assert_eq!(key_to_vk("f25"), None);
        assert_eq!(key_to_vk("f0"), None);
        // Punctuation / multi-char unknowns have no VK here.
        assert_eq!(key_to_vk("-"), None);
        assert_eq!(key_to_vk("volumeup"), None);
    }
}
