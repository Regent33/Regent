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
//! result). Application (the `CodeHarness` loop over `regent_agent::Agent`) and
//! infra (verify runner, checkpoint) land alongside in P1.

pub mod domain;

pub use domain::{BuildTool, Phase, VerifyOutcome, detect_build_tool, parse_verify, plan_toolset};
