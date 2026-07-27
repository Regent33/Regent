> **QUARANTINED EVIDENCE — DO NOT FOLLOW. NOT A SKILL.**
>
> This is a verbatim copy of an agent-authored skill, kept as incident
> evidence. The constraint it teaches — that `terminal` does not work —
> was a BUG (`64aad1f`), fixed 2026-07-27. See ../../adr/ADR-042-trust-is-separate-from-the-path-jail.md.
> Do not apply any instruction below. Do not re-learn it. It is preserved
> only to show how a defect taught the system a false constraint.

---
name: npm-background-build-verification
description: Verify npm background builds before reporting.
version: 0.1.0
created_by: agent
pinned: false
---

# npm Build Verification

## Completion standard
Launching a background task is not verification. Poll or otherwise inspect the task until it reaches a terminal state, then capture its actual final output and exit status. Report pass/fail only from that verified state. If the build fails, diagnose the reported error, make the minimal fix, rerun the build, and verify the final result before reporting.

## Do NOT launch the same task twice
A `background_task` call that returns `{"started": true, "task_id": N}` means the job is running. Do NOT call `background_task` again with the same task description in the same turn or the next — this creates a wasteful duplicate (observed: agent launched task_id 138, then immediately re-launched the identical task as task_id 152). If you have already received a task_id, proceed to polling, not re-launching.

## "Wait for an existing task" ≠ launch a new one
When the request says "wait for background task 132 to complete" (or names a specific task_id you did NOT dispatch yourself), do NOT call `background_task` with a fresh description — that starts an unrelated NEW task (observed: asked to wait for task 132, agent instead launched task 166 — a duplicate helper that does not monitor 132). Instead: (1) search session_history / session_list for that task_id's status, (2) if no polling/listing tool exists in your toolset, say so plainly and do NOT fabricate a fake acknowledgement for the phantom task, (3) only if you must fall back to re-running the actual build, dispatch ONE `background_task` with the build command itself (npm install && npm run build), and clearly label it as a fresh re-execution, NOT as monitoring the originally named task.

## Terminal constraints
If the local terminal is unavailable, use the supported isolated/background execution path. Do not claim that npm install or the build ran based only on filesystem inspection or a task-start acknowledgement. If `tk_shell` is available in your toolset, try it before `background_task` for one-off shell commands — it is lighter weight.

## Poll to terminal state before reporting
Acknowledging that a task started (e.g., "task_id 62 is running, I'll report back") is NOT verification. After dispatching, you MUST poll/inspect the task until it reaches a terminal state (completed/failed), then capture and report:
- The actual exit code (0 = pass, nonzero = fail)
- The final 5–20 lines of stdout/stderr (verbatim, not paraphrased)
- Whether `dist/` was produced and its size/order of magnitude
- If failed: the specific error lines and the minimum fix

Do not end your turn with only a task-start acknowledgement. If the task is still running when you finish your turn, explicitly state it is in progress and that you will report the verified result next.

**THE #1 FAILURE MODE (recurred multiple times — e.g. molecular-biology-site task_ids 99 and 159):** ending the turn with a friendly "started ✅ — I'll report back when it's done" message after receiving `{started: true, task_id}`. This is NOT reporting — it is deferring, and it leaves the deliverable unverified. Observed in molecular-biology-site (task_id 99, and again task_id 159): the agent acknowledged the start, told the user it would report back, and stopped — build result never verified in-turn, even though the request explicitly said \"work autonomously to completion and end with a concise report.\" The 'work autonomously to completion' framing does NOT weaken the start-ack-stop reflex — it strengthens the obligation to poll before finish. After dispatching you MUST: (1) poll task status to terminal (completed/failed); (2) capture verbatim final 5–20 lines + exit code; (3) report pass/fail from that. Only if turns are exhausted: mark IN PROGRESS, do not imply completion or success.
