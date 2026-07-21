//! Suppress the console window Windows flashes for a subprocess.
//!
//! `CREATE_NO_WINDOW` hides only the CONSOLE window — stdout/stderr pipes still
//! work, and it never hides a GUI window the child opens (explorer, a media
//! player, a browser). So it is safe to apply to EVERY spawn: console tools
//! stop flashing a black window, GUI launches are unaffected. No-op off Windows.

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply to a `std::process::Command` before spawning. Returns the command for
/// chaining.
#[cfg(windows)]
pub(crate) fn hide_std(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
pub(crate) fn hide_std(cmd: &mut std::process::Command) -> &mut std::process::Command {
    cmd
}

/// Apply to a `tokio::process::Command` before spawning. Returns the command for
/// chaining.
#[cfg(windows)]
pub(crate) fn hide_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
pub(crate) fn hide_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    cmd
}
