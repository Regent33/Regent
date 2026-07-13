//! Infra for the harness: the verify runner (spawns the detected test/build
//! command), the git checkpoint that backs revert-to-last-green, and the
//! edit-time diagnostics decorator.

mod checkpoint;
mod diagnostics;
mod verify;

pub use checkpoint::GitCheckpoint;
pub use diagnostics::{Diagnostics, wrap_diagnostics};
pub use verify::VerifyRunner;
