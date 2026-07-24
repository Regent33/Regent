//! The Linux `.desktop` file: written by `render`, read back by
//! `dir_from_desktop_entry`. They live together because the shape of the Exec
//! line is load-bearing in both directions — the parser ends at
//! `remove_dir_all`, so a writer/reader drift would delete the wrong directory.
//! Linux-only; the module is gated off elsewhere.

/// The menu entry. `env VAR=… exe` rather than a bare Exec: the app resolves the
/// deacon via REGENT_DEACON_PATH, and a desktop launcher inherits none of the
/// user's shell profile (see `pin_deacon`). Both arguments are quoted per the
/// desktop-entry spec — the install dir is typed by hand, and a space would
/// otherwise split the Exec line into nonsense.
///
/// This file is also how Setup finds the install *again*: Linux has no
/// Apps & features, so `dir_from_desktop_entry` below reads the directory back
/// out when the same AppImage is re-run and offers to uninstall.
pub(super) fn render(install_dir: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Regent\nComment=Built to serve\n\
         Exec=env {} {}\nTerminal=false\nCategories=Development;\n",
        exec_quote(&format!(
            "REGENT_DEACON_PATH={}",
            super::deacon_path(install_dir).display()
        )),
        exec_quote(&super::app_exe(install_dir).display().to_string()),
    )
}

/// One Exec argument, quoted. The spec reserves four characters inside a
/// double-quoted argument; each gets a backslash.
fn exec_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The install directory, recovered from the Exec line `render` wrote.
///
/// Anchored on the known `/bin/regent-deacon"` tail rather than split on
/// whitespace, so a directory with spaces survives. The closing quote in the
/// pattern is what makes `find` safe: a literal `"` inside the path would have
/// been escaped to `\"` by `exec_quote`, so the first unescaped match is
/// always the real end of the first argument. Anything malformed returns None
/// — the caller falls back to the default location and validates either way.
pub(crate) fn dir_from_desktop_entry(entry: &str) -> Option<String> {
    let line = entry
        .lines()
        .find_map(|l| l.strip_prefix("Exec=env \"REGENT_DEACON_PATH="))?;
    let end = line.find("/bin/regent-deacon\"")?;
    let mut escaped = line[..end].chars();
    let mut dir = String::new();
    while let Some(c) = escaped.next() {
        dir.push(if c == '\\' { escaped.next()? } else { c });
    }
    Some(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Written by one function and parsed back by another, and the parse result
    // ends at remove_dir_all — so the pair is tested as a round trip, hostile
    // paths included.
    #[test]
    fn round_trips_the_install_dir() {
        for dir in [
            "/home/x/.local/share/Regent",
            "/home/o'brien/My Apps/Regent",     // spaces + quote
            "/home/x/pa\"th/$HOME/`v\\eird",    // every escaped character
            "/opt/bin/regent-deacon-lookalike", // contains the anchor text
        ] {
            assert_eq!(
                dir_from_desktop_entry(&render(dir)).as_deref(),
                Some(dir),
                "round trip failed for {dir}"
            );
        }
    }

    #[test]
    fn parse_rejects_what_it_does_not_recognise() {
        assert_eq!(dir_from_desktop_entry(""), None);
        assert_eq!(
            dir_from_desktop_entry("[Desktop Entry]\nExec=firefox\n"),
            None
        );
        // The pre-quoting format this installer used to write: unparseable on
        // purpose — the caller falls back to the default location instead.
        assert_eq!(
            dir_from_desktop_entry(
                "Exec=env REGENT_DEACON_PATH=/x/bin/regent-deacon /x/app/Regent\n"
            ),
            None
        );
    }
}
