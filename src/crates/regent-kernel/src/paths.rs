//! Path shapes that matter when handing a directory to a CHILD PROCESS.

use std::path::{Path, PathBuf};

/// Windows extended-length prefix that `std::fs::canonicalize` prepends.
const EXTENDED: &str = r"\\?\";
/// The one extended form that is a genuine network path — stripping this would
/// turn `\\?\UNC\server\share` into the meaningless `UNC\server\share`.
const EXTENDED_UNC: &str = r"\\?\UNC\";

/// A path safe to use as a child process's working directory.
///
/// `std::fs::canonicalize` returns the extended-length form on Windows
/// (`\\?\D:\motivfy`). PowerShell tolerates it, but a great many real tools do
/// not: anything that shells out through CMD.EXE — `flutter`, `npm`, and every
/// other `.bat`/`.cmd` shim — prints
///
/// ```text
/// '\\?\D:\motivfy'
/// CMD.EXE was started with the above path as the current directory.
/// UNC paths are not supported.  Defaulting to Windows directory.
/// ```
///
/// …and then runs in `C:\Windows`, where the project is not. Reported 2026-07-30
/// as `flutter clean` failing with "No pubspec.yaml file found" inside a terminal
/// whose prompt showed the right folder.
///
/// Applied ONLY at the spawn boundary, deliberately. The canonical extended form
/// stays everywhere else because the sandbox jail compares candidate paths
/// against the canonical root by prefix — de-UNC-ing the stored root would make
/// every one of those comparisons fail and refuse all file access.
///
/// A no-op on Unix, and a no-op for genuine `\\?\UNC\` network paths.
#[must_use]
pub fn spawn_cwd(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if !text.starts_with(EXTENDED) || text.starts_with(EXTENDED_UNC) {
        return path.to_path_buf();
    }
    let rest = &text[EXTENDED.len()..];
    // Only a drive-letter path is safe to shorten: `D:\…`. Anything else keeps
    // the prefix rather than being guessed at.
    let looks_like_drive = {
        let bytes = rest.as_bytes();
        bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
    };
    if looks_like_drive {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extended_drive_prefix_is_removed() {
        assert_eq!(
            spawn_cwd(Path::new(r"\\?\D:\motivfy")),
            PathBuf::from(r"D:\motivfy")
        );
        assert_eq!(spawn_cwd(Path::new(r"\\?\C:\")), PathBuf::from(r"C:\"));
    }

    /// `\\?\UNC\server\share` really is a network path; stripping the prefix
    /// yields `UNC\server\share`, which resolves to nothing.
    #[test]
    fn a_genuine_network_path_is_left_alone() {
        let unc = Path::new(r"\\?\UNC\server\share\proj");
        assert_eq!(spawn_cwd(unc), unc.to_path_buf());
    }

    #[test]
    fn ordinary_paths_pass_through_untouched() {
        for p in [
            r"D:\motivfy",
            "/home/ralph/proj",
            r"\\server\share",
            "relative/dir",
        ] {
            assert_eq!(spawn_cwd(Path::new(p)), PathBuf::from(p), "{p}");
        }
    }

    /// An extended prefix on something that is not a drive path is left as-is
    /// rather than mangled into a guess.
    #[test]
    fn an_unrecognised_extended_form_keeps_its_prefix() {
        let odd = Path::new(r"\\?\Volume{1234}\data");
        assert_eq!(spawn_cwd(odd), odd.to_path_buf());
    }
}
