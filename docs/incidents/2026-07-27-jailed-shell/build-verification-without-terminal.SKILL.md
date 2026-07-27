> **QUARANTINED EVIDENCE — DO NOT FOLLOW. NOT A SKILL.**
>
> This is a verbatim copy of an agent-authored skill, kept as incident
> evidence. The constraint it teaches — that `terminal` does not work —
> was a BUG (`64aad1f`), fixed 2026-07-27. See ../../adr/ADR-042-trust-is-separate-from-the-path-jail.md.
> Do not apply any instruction below. Do not re-learn it. It is preserved
> only to show how a defect taught the system a false constraint.

---
name: build-verification-without-terminal
description: Verify any build without terminal access.
version: 0.1.0
created_by: agent
pinned: false
---

Source-agnostic verification of any build/install/test command you cannot run directly.

== When to use ==
- `terminal` returns "terminal is unavailable in this jailed session …"
- `code_task` rejects nesting ("a code task is already running — finish it directly")
- You need to confirm whether a previous background task's build succeeded.
- You were asked to WAIT for / monitor a specific already-running task by its task_id.

== "Wait for task N" ≠ run a fresh build ==
If the request says "wait for background task 132 to complete" (a task_id you did NOT dispatch yourself), the goal is to OBSERVE that task's terminal state, NOT to launch a new one. Dispatching a fresh `background_task` with a similar description creates a DUPLICATE unrelated task (observed: asked to wait for 132, agent launched task 166 — a new helper, not a poll of 132). Correct move: search session_history / session_list for the named task_id's status; if no listing/polling tool exists in your toolset, say so and do NOT fabricate a "started" acknowledgement for the phantom task. Only fall back to dispatching ONE fresh `background_task(NPM install && build)` if you genuinely need to RUN the build yourself — and label it as a fresh re-execution, not monitoring the named task.

== Plan-mode / read-only exception ==
When the task is explicitly framed as plan mode ("READ-ONLY phase", "you MUST NOT make any edits or run any mutating tools", "only read-only tools are available"), dispatching `background_task` (which runs `npm install` + `vite build`) IS a mutating action and is prohibited by the user's constraint — the.Dispatch-first rule below does NOT apply. In that case the correct deliverable is a thorough static pre-flight of the import/export graph, configs, and dependency versions (exactly the read_file/glob/ls inspection pattern), ending with a plan that says "build is expected to pass; run it when plan mode lifts." This is NOT the anti-pattern — distinguish it: the anti-pattern is exhaustive-only file reading when mutation IS permitted and an execution path exists. Session confirmed: molecular-biology-site plan-mode verification legitimately read package.json, vite.config.js, tailwind.config.js, postcss.config.js, index.html, main.jsx, App.jsx, router.jsx, MainLayout.jsx, HeroBackground.jsx, hero features data, and the three-utils — the import/export graph was sound, chunking intact, R149-era Three.js symbols present, no require() found; build expected clean.

== Anti-pattern ==
- Do NOT claim the build passed based on file inspection alone.
- Do NOT trust a `delegate_task`/subagent report naming specific code defects — verify each cited file with `read_file` before acting.
- Do NOT fall straight from a jail-blocked `terminal` to a static-only report when mutation is ALLOWED. Static inspection is a *supplement*, not a substitute, for actually running the command. If you finish the task with "BUILD FAILED — BLOCKED" without ever dispatching `background_task` (and plan mode / read-only constraints are NOT in force), you have skipped the one execution path that almost always works.

== Pre-check (before step 1) ==
Inspect your OWN toolset: if `background_task` is NOT among your available tools, you cannot dispatch one. Skip straight to the honesty rule/static-inspection fallback and report plainly that the build could not be run. Do NOT call `background_task` and pretend it started — a call to a phantom tool produces nothing.

== Recipe (order matters) ==
1. **Dispatch `background_task` immediately — BEFORE any other step.** The moment `terminal` returns the jailed-session error, dispatch via `background_task` in the same or next tool block. Do NOT first walk the source tree, read package.json, list features, or inspect configs — that inspection is step 2 and runs in PARALLEL after dispatch, not before. Observed (molecular-biology-site, repeated): the agent read package.json, vite.config, index.html, main.jsx, App.jsx, router.jsx, all 6 feature index.js files, three-utils.js, UIComponents, MainLayout, tailwind/postcss configs — over a dozen read_file/ls calls — then the turn ended with no `background_task` ever dispatched. That is the failure this step prevents. Dispatch first:
     background_task(label="verify npm build", exec="cd \"C:\\path\\to\\project\" && npm install --no-audit --no-fund && npm run build")
   `background_task` runs in an isolated execution environment and is not subject to the local shell jail. Do NOT block the turn waiting on it; continue with step 2 while it runs.
2. While `background_task` runs, do parallel static pre-flight with file tools (they respect the filesystem jail):
   - `ls` on the project root + `src/` + `node_modules/` + `dist/`
   - `read_file` on `package.json` (scripts, deps), the relevant config (vite/webpack/etc), and entry points
   - `glob` for `dist/**/*` and `node_modules/.package-lock.json` — empty both means no previous build ran
   - `grep` for the specific error signatures you suspect
   Static findings help you anticipate fixes; they do not replace running the build.
3. When `background_task` returns, poll/inspect it to a terminal state, then report:
   - exit code (0 = pass, nonzero = fail)
   - the actual final 5–20 lines of stdout/stderr (not a paraphrase)
   - whether `dist/` was produced and its size/order of magnitude
   - if failed: the specific error lines and the minimum fix
4. Only if `background_task` is ALSO unavailable should you fall back to pure static inspection, and in that case you must say so plainly.

== Honesty rule ==
If no execution path (terminal, code_task, background_task) is available, say so plainly. Report separately:
  - "Verified by static inspection: [list]"
  - "Could not verify: [npm install exit code, vite build exit code, dist output, runtime behavior]"

== Cross-reference ==
- `jailed-terminal-fallback` (file-level fallback)