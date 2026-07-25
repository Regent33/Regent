//! Infra for the harness: the verify runner (spawns the detected test/build
//! command), the git checkpoint that backs revert-to-last-green, the
//! read/write git operations a workspace surface needs (status/diff/commit/
//! push), and the edit-time diagnostics decorator.

mod checkpoint;
mod diagnostics;
mod git_ops;
mod verify;

pub use checkpoint::GitCheckpoint;
pub use diagnostics::{Diagnostics, wrap_diagnostics};
pub use git_ops::{GitStatus, GitStatusEntry, git_commit, git_diff, git_push, git_status};
pub use verify::VerifyRunner;
