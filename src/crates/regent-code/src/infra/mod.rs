//! Infra for the harness: the verify runner (spawns the detected test/build
//! command) and the git checkpoint that backs revert-to-last-green.

mod checkpoint;
mod verify;

pub use checkpoint::GitCheckpoint;
pub use verify::VerifyRunner;
