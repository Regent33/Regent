//! Which flow this process runs (install vs uninstall) and where. Routed on the
//! executable's own file name, not an argument: Apps & features invokes the
//! UninstallString with no args, and a user who double-clicks `uninstall.exe`
//! in Explorer passes none either.

use serde::Serialize;

/// Which flow this process is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Install,
    Uninstall,
}

/// Routed on the executable's own file name, not on an argument.
pub(crate) fn mode() -> Mode {
    let stem = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
    mode_for(
        stem.as_deref(),
        std::env::args().any(|a| a == "--uninstall"),
    )
}

/// Split out from `mode` so the routing can be tested — getting this backwards
/// means Apps & features opens the installer.
fn mode_for(exe_stem: Option<&str>, uninstall_flag: bool) -> Mode {
    // The flag is for `tauri dev`, where the binary is always regent-installer.
    let named = exe_stem.is_some_and(|n| n.eq_ignore_ascii_case("uninstall"));
    if named || uninstall_flag {
        Mode::Uninstall
    } else {
        Mode::Install
    }
}

/// What the frontend needs before it can render: which flow, and the directory
/// it concerns. One call rather than two so there is a single point at which the
/// UI knows what it is. `existing_install` is an install already on this machine,
/// spotted while in Install mode (macOS/Linux only — Windows lets Apps & features
/// own that door); the UI then offers to remove it instead of installing over it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Startup {
    pub(crate) mode: Mode,
    pub(crate) install_dir: String,
    pub(crate) existing_install: Option<String>,
}

#[cfg(windows)]
pub(crate) fn existing_install() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub(crate) fn existing_install() -> Option<String> {
    super::uninstall::detect_install().map(|p| p.display().to_string())
}

/// Per-user default install directory (no elevation required).
pub(crate) fn default_install_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(|p| format!("{p}\\Programs\\Regent"))
            .unwrap_or_else(|_| "C:\\Program Files\\Regent".into())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|h| format!("{h}/.local/share/Regent"))
            .unwrap_or_else(|_| "/opt/Regent".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_routes_on_the_executable_name() {
        // What Apps & features runs: our own copy, no arguments.
        assert_eq!(mode_for(Some("uninstall"), false), Mode::Uninstall);
        // file_stem() drops ".exe", and Windows filenames are case-insensitive.
        assert_eq!(mode_for(Some("Uninstall"), false), Mode::Uninstall);
        // Anything else is a setup run — including the dev binary.
        assert_eq!(mode_for(Some("regent-installer"), false), Mode::Install);
        assert_eq!(mode_for(Some("Regent Setup"), false), Mode::Install);
        assert_eq!(mode_for(None, false), Mode::Install);
        // `tauri dev` can only reach uninstall through the flag.
        assert_eq!(mode_for(Some("regent-installer"), true), Mode::Uninstall);
    }

    #[test]
    #[cfg(windows)]
    fn uninstaller_name_matches_what_mode_routes_on() {
        // These two drifting apart is silent: wire copies to one name and the
        // router looks for another, so Apps & features opens the installer.
        let stem = std::path::Path::new(crate::wire::UNINSTALLER_NAME)
            .file_stem()
            .and_then(|s| s.to_str());
        assert_eq!(mode_for(stem, false), Mode::Uninstall);
    }
}
