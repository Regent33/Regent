//! E2E agent-loop behavior with a scripted provider, a real tool catalog,
//! and a real on-disk store: prompt → parallel tool calls → final answer,
//! plus the harness stop conditions (budget, interrupt) and session resume.
//! One test binary; the cases live in per-behavior modules.

mod helpers;
mod interrupts;
mod resume;
mod turn_flow;
