use crate::domain::contracts::{ApprovalHandler, PermissionRule};
use regent_kernel::RegentError;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Per-dispatch execution context handed to every executor.
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub approval: Arc<dyn ApprovalHandler>,
    /// When set, the filesystem sandbox roots: every path a tool `resolve`s
    /// must stay within ONE of them (`..` traversal, symlink escapes in the
    /// existing prefix, and out-of-root absolute paths are rejected). `None`
    /// leaves filesystem access unrestricted — the local-dev default.
    sandbox: Option<Vec<PathBuf>>,
    /// Gap T6: where oversized tool results spill in full (the model gets the
    /// head + a receipt path). `None` = truncate without spill, head only.
    /// For jailed sessions this must sit inside an allowed subtree so the
    /// model can `read_file` the receipt.
    pub scratch_dir: Option<PathBuf>,
    /// Bug #10: where produced artifacts (documents, exports) land when the
    /// model gives a RELATIVE path — `$REGENT_HOME/artifacts` in the deacon.
    /// `None` = relative paths resolve against `cwd` (the old behavior; the
    /// CLI/repl and tests don't set one).
    pub artifacts_dir: Option<PathBuf>,
    /// Gap S5: permission rules evaluated per dispatch (last match wins).
    /// Empty = no rules = today's behavior exactly.
    pub permission_rules: Arc<[PermissionRule]>,
    /// Whether this session's INPUT is untrusted — an external platform turn
    /// (webhook/gateway) or an explicit `REGENT_SANDBOX` run. Deliberately
    /// SEPARATE from `sandbox`: jailing paths is a safety rail every session
    /// wants, while "somebody outside chose these words" is a much narrower
    /// claim. `64aad1f` made the path jail default-on and two call sites were
    /// reading `is_sandboxed()` to mean this, which silently took the local
    /// shell and direct memory writes away from every ordinary session.
    untrusted: bool,
    /// The turn's interrupt, when the caller has one. Carried HERE because the
    /// context is the only per-dispatch channel a tool sees: a `delegate_task`
    /// child is a whole agent of its own, and without this it got a fresh token
    /// and never learned the parent had been stopped. See
    /// `ToolContext::cancel_token`.
    cancel: Option<CancellationToken>,
    /// The artifacts subfolder this session's documents share, claimed by the
    /// first one to need it.
    ///
    /// A context is built per SESSION, so this cell is what makes "a PDF and a
    /// deck asked for together land together" a guarantee rather than a hope.
    /// It used to depend on the model passing matching paths, and it did not:
    /// a deck went to `black-panther-wakanda-forever-presentation/` while its
    /// companion PDF stayed loose in the artifacts root.
    doc_folder: Arc<std::sync::Mutex<Option<String>>>,
}

impl ToolContext {
    #[must_use]
    pub fn new(cwd: PathBuf, approval: Arc<dyn ApprovalHandler>) -> Self {
        Self {
            cwd,
            approval,
            sandbox: None,
            scratch_dir: None,
            artifacts_dir: None,
            permission_rules: Arc::from(Vec::new()),
            untrusted: false,
            cancel: None,
            doc_folder: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Like [`ToolContext::new`] but jails every resolved path under `root`.
    /// Every session gets this — it stops a hallucinated or injected absolute
    /// path from reaching outside the working tree. It does NOT by itself mean
    /// the session is untrusted; call [`ToolContext::untrusted`] for that.
    #[must_use]
    pub fn new_sandboxed(cwd: PathBuf, root: PathBuf, approval: Arc<dyn ApprovalHandler>) -> Self {
        Self {
            cwd,
            approval,
            sandbox: Some(vec![root]),
            scratch_dir: None,
            artifacts_dir: None,
            permission_rules: Arc::from(Vec::new()),
            untrusted: false,
            cancel: None,
            doc_folder: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// The artifacts subfolder every document in this session shares.
    ///
    /// The first caller's `proposed` name wins and is remembered; every later
    /// caller gets that same folder back regardless of what it proposed. A
    /// poisoned lock falls back to the proposal rather than panicking — a
    /// document landing in its own folder is a far better outcome than a tool
    /// that refuses to write one.
    pub fn document_folder(&self, proposed: &str) -> String {
        match self.doc_folder.lock() {
            Ok(mut slot) => slot.get_or_insert_with(|| proposed.to_owned()).clone(),
            Err(_) => proposed.to_owned(),
        }
    }

    /// Marks the session's input as untrusted: an external platform turn, or
    /// an explicit `REGENT_SANDBOX` run. Tools that must not act on somebody
    /// else's words — the local shell, direct memory writes — key off this.
    #[must_use]
    pub fn untrusted(mut self) -> Self {
        self.untrusted = true;
        self
    }

    /// Sets the spill area for oversized tool results (gap T6).
    #[must_use]
    pub fn with_scratch_dir(mut self, dir: PathBuf) -> Self {
        self.scratch_dir = Some(dir);
        self
    }

    /// Sets where relative-path artifacts (documents, exports) land (bug #10).
    #[must_use]
    pub fn with_artifacts_dir(mut self, dir: PathBuf) -> Self {
        self.artifacts_dir = Some(dir);
        self
    }

    /// Installs permission rules (gap S5) — evaluated on every dispatch,
    /// last match wins.
    #[must_use]
    pub fn with_permission_rules(mut self, rules: Vec<PermissionRule>) -> Self {
        self.permission_rules = Arc::from(rules);
        self
    }

    /// Hands the turn's interrupt to the tools it dispatches. `Agent::new`
    /// stamps its own token here and **adopts** one already present, which is
    /// what links a delegated child to the parent that spawned it.
    #[must_use]
    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// The turn's interrupt, if the caller installed one.
    #[must_use]
    pub fn cancel_token(&self) -> Option<CancellationToken> {
        self.cancel.clone()
    }

    /// Adds an extra allowed subtree to an existing jail (e.g. the
    /// `$REGENT_HOME/artifacts` area, so a jailed external session can still
    /// save its outputs where every other session does). No-op when the
    /// context is unsandboxed — there is nothing to widen.
    #[must_use]
    pub fn allow_subtree(mut self, root: PathBuf) -> Self {
        if let Some(roots) = &mut self.sandbox {
            roots.push(root);
        }
        self
    }

    /// Whether this context jails filesystem access.
    #[must_use]
    pub fn is_sandboxed(&self) -> bool {
        self.sandbox.is_some()
    }

    /// Whether this session's input came from outside the user. NOT the same
    /// question as [`ToolContext::is_sandboxed`] — see the field comment.
    #[must_use]
    pub fn is_untrusted(&self) -> bool {
        self.untrusted
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
            Some(roots) => roots
                .iter()
                .find_map(|root| contained(root, &joined))
                .ok_or_else(|| RegentError::Tool {
                    tool: "sandbox".into(),
                    // Say what to DO, not just what failed. The bare "escapes
                    // the sandbox root" left the model retrying variations of
                    // the same path and the user with no idea the jail was
                    // deliberate or how to widen it.
                    message: format!(
                        "path '{path}' is outside this session's workspace, so it was not touched. \
                         To work there, open that folder for the session (Files → Open Folder in \
                         the desktop app, or start the CLI inside it). Ask the user first — do not \
                         try to reach it another way."
                    ),
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
#[path = "entities_tests.rs"]
mod tests;
