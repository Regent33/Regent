//! Phase prompts for the coding harness (plan / execute / fix). Public —
//! the deacon's code.start frames its turns identically.
//! Split from `harness.rs` (file-size rule).

/// Plan-phase turn text. Applies Claude Code's plan-mode discipline: a hard
/// read-only constraint that supersedes other instructions, explore-and-reuse,
/// and a structured, concise-but-executable plan. Public so other surfaces (the
/// deacon's `code.plan` RPC) frame the plan turn identically.
pub fn plan_prompt(task: &str) -> String {
    format!(
        "Plan mode is active — this is a READ-ONLY phase. You MUST NOT make any edits or run \
         any mutating tools; only the read-only tools (read_file, glob, search_files, ls) are \
         available to you. This supersedes any other instruction to edit.\n\n\
         Task: {task}\n\n\
         Explore the codebase with the read-only tools to understand what's needed, then write a \
         concise, executable PLAN. If this is a BUG FIX, step 1 of the plan is a failing test \
         that reproduces the bug — the fix is done only when that test passes. Structure the \
         plan as:\n\
         - Context — why this change is needed, the problem it addresses\n\
         - Approach — your single recommended approach (not a list of alternatives)\n\
         - Files — the specific files to create or modify\n\
         - Reuse — existing code to build on, with file paths\n\
         - Verification — how to confirm it works (the tests/build to run)\n\n\
         Keep it scannable but detailed enough to execute. Output the plan as your reply."
    )
}

/// Fix-turn text after a red verify (gap H4): the failure output goes back to
/// the SAME execute agent, bounded by `max_fix_attempts`, with the revert
/// backstop unchanged. Public so the deacon's `code.start` loop frames its fix
/// turns identically.
pub fn fix_prompt(summary: &str) -> String {
    format!(
        "Verification failed. Output:\n{summary}\nDiagnose the root cause and fix it. Do not \
         expand scope; do not disable or delete tests to make them pass."
    )
}

/// Execute-phase turn text. The plan is approved; implement it with the full
/// toolset, fix root causes, reuse code, and don't expand scope. Public so the
/// deacon's `code.start` RPC frames the execute turn identically.
pub fn execute_prompt(task: &str, plan: &str) -> String {
    format!(
        "Execute mode — the plan below is APPROVED. Implement it now using your full toolset; \
         don't expand scope beyond the plan. When done, reply with a concise report \
         of what you changed.\n\n\
         Task: {task}\n\n\
         Approved plan:\n{plan}"
    )
}
