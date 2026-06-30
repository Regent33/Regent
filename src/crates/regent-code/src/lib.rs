//! regent-code — the coding-specialized harness over `regent-agent`.
//!
//! Not a re-implementation of the agent loop: a disciplined wrapper that does
//! what Claude Code gets right — **plan-mode gate → edit → per-step verify →
//! revert-to-last-green on failure** — using the editing tools and sandbox that
//! already ship. The model decides; this harness constrains, executes, and
//! verifies.
//!
//! Clean-architecture layout: `domain/` (pure, zero-I/O decisions — which build
//! tool to verify with, which tools plan-mode may touch, how to read a verify
//! result), `application/` (the `CodeHarness` loop over `regent_agent::Agent`),
//! `infra/` (the verify runner and the git checkpoint that backs revert).

pub mod application;
pub mod domain;
pub mod infra;

pub use application::{
    Checkpoint, CodeHarness, CodeOutcome, Verifier, execute_prompt, plan_prompt,
};
pub use domain::{BuildTool, Phase, VerifyOutcome, detect_build_tool, parse_verify, plan_toolset};
pub use infra::{GitCheckpoint, VerifyRunner};
