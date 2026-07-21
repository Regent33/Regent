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

/// VK sequence for a combo that must go through `keybd_event` — either it uses
/// the Windows modifier (SendKeys has none) or its key is one SendKeys cannot
/// express at all (media / browser / context-menu keys). Returns `None` when
/// SendKeys can handle the combo; `Some(Err)` when a key in a keybd_event combo
/// has no VK mapping (a loud error beats a wrong key). On success: (modifier VKs
/// in press order, key VK).
pub(super) fn keybd_combo(combo: &str) -> Option<Result<(Vec<u8>, u8), String>> {
    let parts: Vec<String> = combo.split('+').map(|p| p.trim().to_lowercase()).collect();
    let has_win = parts
        .iter()
        .any(|p| matches!(p.as_str(), "win" | "super" | "meta" | "cmd"));
    let has_keybd_only = parts.iter().any(|p| keybd_only_vk(p).is_some());
    if !has_win && !has_keybd_only {
        return None; // SendKeys handles it
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
                        "can't send '{other}' as a key on this backend"
                    )));
                }
            },
        }
    }
    Some(match key {
        Some(vk) => Ok((modifiers, vk)),
        None => Err("a keyboard shortcut needs a key, e.g. win+r or medianext".into()),
    })
}

/// VK for a media / browser / context-menu key — the ones SendKeys can't
/// produce. Kept as its own set so the routing check above is exactly this.
fn keybd_only_vk(k: &str) -> Option<u8> {
    Some(match k {
        "volumeup" | "volup" => 0xAF,
        "volumedown" | "voldown" => 0xAE,
        "mute" | "volumemute" | "volmute" => 0xAD,
        "medianext" | "nexttrack" => 0xB0,
        "mediaprev" | "prevtrack" | "previoustrack" => 0xB1,
        "mediastop" => 0xB2,
        "mediaplay" | "playpause" | "mediaplaypause" => 0xB3,
        "browserback" => 0xA6,
        "browserforward" => 0xA7,
        "browserrefresh" => 0xA8,
        "browserstop" => 0xA9,
        "browsersearch" => 0xAA,
        "browserfavorites" => 0xAB,
        "browserhome" => 0xAC,
        "apps" | "menu" | "contextmenu" => 0x5D, // VK_APPS (context-menu key)
        _ => return None,
    })
}

/// Virtual-key code for a key name (for the `keybd_event` path). Covers named
/// keys, F1–F24, US-layout OEM punctuation (so Win+. / Win+; work), and the
/// media/browser keys above.
fn key_to_vk(k: &str) -> Option<u8> {
    if let Some(vk) = keybd_only_vk(k) {
        return Some(vk);
    }
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
        "printscreen" | "prtsc" | "prtscr" | "prtscn" => 0x2C,
        "pause" | "break" => 0x13,
        "capslock" => 0x14,
        "numlock" => 0x90,
        "scrolllock" => 0x91,
        // Numpad operators (magnifier Win + Plus/Minus lands on these).
        "add" | "plus" => 0x6B,
        "subtract" | "minus" => 0x6D,
        "multiply" => 0x6A,
        "divide" => 0x6F,
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
            0x70 + (f[1..].parse::<u8>().unwrap() - 1) // VK_F1 = 0x70
        }
        s if s.chars().count() == 1 => single_char_vk(s.chars().next().unwrap())?,
        _ => return None,
    })
}

/// VK for a single character: alphanumerics map to their ASCII-upper VK, and
/// the US-layout OEM punctuation keys map to their VK_OEM_* codes — so a combo
/// like win+. (emoji picker) or win+; resolves instead of erroring.
fn single_char_vk(c: char) -> Option<u8> {
    Some(match c {
        c if c.is_ascii_alphanumeric() => c.to_ascii_uppercase() as u8,
        ' ' => 0x20,
        ';' | ':' => 0xBA,  // VK_OEM_1
        '=' | '+' => 0xBB,  // VK_OEM_PLUS
        ',' | '<' => 0xBC,  // VK_OEM_COMMA
        '-' | '_' => 0xBD,  // VK_OEM_MINUS
        '.' | '>' => 0xBE,  // VK_OEM_PERIOD
        '/' | '?' => 0xBF,  // VK_OEM_2
        '`' | '~' => 0xC0,  // VK_OEM_3
        '[' | '{' => 0xDB,  // VK_OEM_4
        '\\' | '|' => 0xDC, // VK_OEM_5
        ']' | '}' => 0xDD,  // VK_OEM_6
        '\'' | '"' => 0xDE, // VK_OEM_7
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
        "printscreen" | "prtsc" | "prtscr" | "prtscn" => "{PRTSC}".into(),
        "break" | "pause" => "{BREAK}".into(),
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
    fn keybd_combos_map_to_vk_codes_not_sendkeys() {
        // win+r → LWIN modifier + 'R'. Aliases all mean the same key.
        for alias in ["win+r", "super+r", "meta+r", "cmd+r"] {
            assert_eq!(
                keybd_combo(alias).unwrap().unwrap(),
                (vec![0x5B], 0x52),
                "{alias}"
            );
        }
        // Press order preserved; win+shift+s is the Windows snip shortcut.
        assert_eq!(
            keybd_combo("win+shift+s").unwrap().unwrap(),
            (vec![0x5B, 0x10], 0x53)
        );
        // Every modifier before the key: ctrl+alt+win+delete.
        assert_eq!(
            keybd_combo("ctrl+alt+win+delete").unwrap().unwrap(),
            (vec![0x11, 0x12, 0x5B], 0x2E)
        );
        // The combos that USED to fail: Win + OEM punctuation (emoji picker /
        // clipboard peek), Win+Pause, Win + Plus (magnifier).
        assert_eq!(keybd_combo("win+.").unwrap().unwrap(), (vec![0x5B], 0xBE)); // VK_OEM_PERIOD
        assert_eq!(keybd_combo("win+;").unwrap().unwrap(), (vec![0x5B], 0xBA)); // VK_OEM_1
        assert_eq!(keybd_combo("win+,").unwrap().unwrap(), (vec![0x5B], 0xBC)); // VK_OEM_COMMA
        assert_eq!(
            keybd_combo("win+pause").unwrap().unwrap(),
            (vec![0x5B], 0x13)
        );
        assert_eq!(
            keybd_combo("win+plus").unwrap().unwrap(),
            (vec![0x5B], 0x6B)
        );
        assert_eq!(keybd_combo("win+f4").unwrap().unwrap(), (vec![0x5B], 0x73));

        // Media / browser keys route to keybd_event even WITHOUT a modifier —
        // SendKeys would type them as literal text.
        assert_eq!(keybd_combo("volumeup").unwrap().unwrap(), (vec![], 0xAF));
        assert_eq!(keybd_combo("playpause").unwrap().unwrap(), (vec![], 0xB3));
        assert_eq!(keybd_combo("browserback").unwrap().unwrap(), (vec![], 0xA6));

        // Non-Win, non-media combos are None — SendKeys owns those.
        assert!(keybd_combo("ctrl+s").is_none());
        assert!(keybd_combo("enter").is_none());
        assert!(keybd_combo("ctrl+shift+t").is_none());

        // Loud errors, never a wrong key: no key, or only modifiers.
        assert!(keybd_combo("win").unwrap().is_err());
        assert!(keybd_combo("win+ctrl").unwrap().is_err());
    }

    #[test]
    fn key_to_vk_covers_named_punctuation_media_and_f_key_bounds() {
        assert_eq!(key_to_vk("a"), Some(0x41));
        assert_eq!(key_to_vk("z"), Some(0x5A));
        assert_eq!(key_to_vk("0"), Some(0x30));
        assert_eq!(key_to_vk("enter"), Some(0x0D));
        assert_eq!(key_to_vk("pgup"), Some(0x21));
        assert_eq!(key_to_vk("pause"), Some(0x13));
        assert_eq!(key_to_vk("capslock"), Some(0x14));
        // OEM punctuation now resolves (was None → the Win+. failure).
        assert_eq!(key_to_vk("."), Some(0xBE));
        assert_eq!(key_to_vk(";"), Some(0xBA));
        assert_eq!(key_to_vk("/"), Some(0xBF));
        assert_eq!(key_to_vk("["), Some(0xDB));
        // Media/browser keys via the shared keybd-only set.
        assert_eq!(key_to_vk("volumeup"), Some(0xAF));
        assert_eq!(key_to_vk("browserrefresh"), Some(0xA8));
        // VK has F1 (0x70) through F24 (0x87); F25 and F0 are out of range.
        assert_eq!(key_to_vk("f1"), Some(0x70));
        assert_eq!(key_to_vk("f24"), Some(0x87));
        assert_eq!(key_to_vk("f25"), None);
        assert_eq!(key_to_vk("f0"), None);
        // Truly unmappable: a random word.
        assert_eq!(key_to_vk("wat"), None);
    }
}
