use crate::domain::contracts::ApprovalHandler;
use regent_kernel::RegentError;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Per-dispatch execution context handed to every executor.
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub approval: Arc<dyn ApprovalHandler>,
    /// When set, the filesystem sandbox root: every path a tool `resolve`s must
    /// stay within it (`..` traversal, symlink escapes in the existing prefix,
    /// and out-of-root absolute paths are rejected). `None` leaves filesystem
    /// access unrestricted — the local-dev default.
    sandbox: Option<PathBuf>,
}

impl ToolContext {
    #[must_use]
    pub fn new(cwd: PathBuf, approval: Arc<dyn ApprovalHandler>) -> Self {
        Self {
            cwd,
            approval,
            sandbox: None,
        }
    }

    /// Like [`ToolContext::new`] but jails every resolved path under `root`.
    /// Used when `REGENT_SANDBOX` is enabled so externally-triggered turns
    /// (chat platforms, webhooks) can't read or write outside the workspace.
    #[must_use]
    pub fn new_sandboxed(cwd: PathBuf, root: PathBuf, approval: Arc<dyn ApprovalHandler>) -> Self {
        Self {
            cwd,
            approval,
            sandbox: Some(root),
        }
    }

    /// Whether this context jails filesystem access.
    #[must_use]
    pub fn is_sandboxed(&self) -> bool {
        self.sandbox.is_some()
    }

    /// Resolves a tool-supplied path against the context cwd, enforcing the
    /// sandbox jail when one is set. Returns an error if the path escapes the
    /// sandbox root.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, RegentError> {
        let candidate = Path::new(path);
        let joined = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            self.cwd.join(candidate)
        };
        match &self.sandbox {
            None => Ok(joined),
            Some(root) => contained(root, &joined).ok_or_else(|| RegentError::Tool {
                tool: "sandbox".into(),
                message: format!("path '{path}' escapes the sandbox root"),
            }),
        }
    }
}

/// Returns the canonical form of `candidate` iff it stays within `root`, else
/// `None`. `..` traversal is rejected outright (so it can't slip past via a
/// not-yet-existing tail); the longest existing prefix is canonicalized so
/// symlink escapes within it are caught, and the not-yet-created remainder is
/// re-appended (a write to a new file is still contained).
fn contained(root: &Path, candidate: &Path) -> Option<PathBuf> {
    if candidate.components().any(|c| c == Component::ParentDir) {
        return None;
    }
    let canon_root = root.canonicalize().ok()?;
    let mut prefix = candidate.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let canon_prefix = loop {
        if let Ok(canon) = prefix.canonicalize() {
            break canon;
        }
        tail.push(prefix.file_name()?.to_os_string());
        if !prefix.pop() {
            return None;
        }
    };
    let mut full = canon_prefix;
    for name in tail.iter().rev() {
        full.push(name);
    }
    full.starts_with(&canon_root).then_some(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::contracts::DenyAll;

    fn ctx_sandboxed(root: &Path) -> ToolContext {
        ToolContext::new_sandboxed(root.to_path_buf(), root.to_path_buf(), Arc::new(DenyAll))
    }

    #[test]
    fn unsandboxed_resolve_is_unrestricted() {
        let cwd = std::env::temp_dir();
        let ctx = ToolContext::new(cwd.clone(), Arc::new(DenyAll));
        assert!(!ctx.is_sandboxed());
        // A relative path joins to cwd; resolution never errors without a jail.
        assert_eq!(ctx.resolve("a/b.txt").unwrap(), cwd.join("a/b.txt"));
    }

    #[test]
    fn sandbox_allows_paths_inside_root_and_rejects_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ctx = ctx_sandboxed(root);
        assert!(ctx.is_sandboxed());

        // Inside the root (existing dir + a not-yet-created file) is allowed.
        let inside = ctx.resolve("sub/new.txt").expect("inside root");
        assert!(inside.starts_with(root.canonicalize().unwrap()));

        // `..` traversal is rejected (platform-independent).
        assert!(ctx.resolve("../escape.txt").is_err());
        assert!(ctx.resolve("sub/../../escape.txt").is_err());

        // An absolute path outside the root is rejected (built from the root's
        // parent so it's genuinely absolute on every platform).
        let outside = root.parent().unwrap().join("outside.txt");
        assert!(ctx.resolve(outside.to_str().unwrap()).is_err());
    }
}
