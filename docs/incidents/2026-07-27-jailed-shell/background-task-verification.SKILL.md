> **QUARANTINED EVIDENCE — DO NOT FOLLOW. NOT A SKILL.**
>
> This is a verbatim copy of an agent-authored skill, kept as incident
> evidence. The constraint it teaches — that `terminal` does not work —
> was a BUG (`64aad1f`), fixed 2026-07-27. See ../../adr/ADR-042-trust-is-separate-from-the-path-jail.md.
> Do not apply any instruction below. Do not re-learn it. It is preserved
> only to show how a defect taught the system a false constraint.

---
name: background-task-verification
description: Verify asynchronous command tasks before reporting.
version: 0.1.0
created_by: agent
pinned: false
---

Use this when a requested shell command cannot run in the foreground and an asynchronous/background execution path is available.

1. Launch the task ONCE with the exact requested shell, working directory, and command sequence. If the call returns `{"started": true, "task_id": N}`, the job is running — do NOT re-invoke `background_task` with the same task description. A duplicate launch wastes resources and can create conflicting concurrent jobs (observed: agent received task_id 138 then immediately re-launched identical task as task_id 152). Proceed to step 2.
2. Poll or otherwise retrieve the task result before composing a report; a launch acknowledgement is not execution evidence.
3. Inspect the complete captured stdout and stderr, exit status, retry outcomes, and any requested artifact listings.
4. If the task cannot be retrieved or execution is unavailable, report that limitation plainly. Never infer that the background environment shares the foreground sandbox, and never present file inspection, package metadata, or expected errors as command output.
5. JAILED TERMINAL: if the foreground `terminal` tool returns "terminal is unavailable in this jailed session", do NOT reflexively fall back to `background_task` for the same shell command. Observed: `background_task` returns only a generic `{started:true, task_id:N}` acknowledgement on every call and never surfaces real stdout/stderr/exit-code — polling it re-issues identical acknowledgements (task_id 123→126→129→135→150…) and triggers the "identical call 3x" guard. That path cannot execute or capture a real build. Halting and reporting the limitation plainly is correct; creating nested "wait for the task" jobs does not help because there is no underlying executor. Only use `background_task` when CONFIRMED to actually run and return real output in the current environment.
5a. `control_app` (PowerShell) is ALSO blocked in BACKGROUND sessions: returns "denied by approval policy" because no user is present to approve it. So the jailed-background fallback ladder is: terminal (jail error) → control_app (approval denied) → background_task (unreliable output). The one reliably-working path in a jailed background session is STATIC SOURCE REVIEW via file/read/list tools (they respect the jail): proactively read all source files to catch missing imports, JSX in .js files (needs Vite optimizeDeps esbuildOptions loader .js→jsx), Three.js OrbitControls ESM import issues, and Tailwind/Octicon syntax errors — while any async executor runs in parallel.
6. Keep the user's constraints intact: do not modify source files, do not install unrequested runtimes, preserve path quoting, and include verbatim logs when requested.