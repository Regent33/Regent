# Regent Code v2 — improvement plan

*2026-07-13. Grounded in three study notes: [opencode](../research/opencode-study.md),
[Claude Code](../research/claude-code-study.md), and
[external research](../research/coding-agent-methods.md). Plan only — nothing here is implemented.
Every recommendation names the files it would change and how; all new code respects the canonical
crate layering (application / domain / infra) and the ~200-line file cap.*

---

## 1. Where Regent stands today

A precise inventory, because the plan's whole argument rests on it.

**The loop** (`regent-agent/src/application/agent/turn.rs`): one user turn = push user message →
loop { check cancel / `max_iterations` (default 90) / optional `max_turn_tokens` ceiling → prune →
maybe-compress → model call (streaming or not, cancellable via `tokio::select!`) → persist → if no
tool calls, return text → dispatch ALL tool calls in parallel (`join_all`), re-attach results in
call order → persist each }. Failed/interrupted turns settle pending tools with synthetic results
and drop the trailing user message; resume replays the store through an alternation-validating
transcript and repairs illegal rows. Cache-reset attribution (routing > compaction > failover >
pruning) is recorded per turn (SPL P2/P3).

**Context** (`regent-agent/src/domain/compression.rs` + `application/lifecycle.rs`): two layers.
Tool-result pruning stubs results older than `prune_after_turns` (5) user-turns, protecting the
newest `protect_last_n` (20) messages, batched behind a 2k-token floor so each prune pays for the
cache reset it forces. Compression triggers at `trigger_fraction` (0.5) of `max_context_tokens`
(128k default): summarize the middle, keep the newest N verbatim, split into a **child session**
(history is never mutated in place). Graph memory captures an episode at the split.

**The coding harness** (`regent-code`): `CodeHarness::run(task)` = plan phase (fresh agent whose
catalog is physically restricted to a read-only allowlist — `plan_toolset`, 11 tools) → approval
gate through the surface's `ApprovalHandler` (deny = nothing executed) → git snapshot
(`infra/checkpoint.rs`; `None` degrades revert to report-only) → execute phase (fresh agent, full
catalog) → verify (`infra/verify.rs` detects cargo/npm/make/pytest from root manifests and runs
the lane) → red verify ⇒ restore snapshot. `code_task`
(`regent-deacon/src/application/code_task_tool.rs`) lets the chat model route itself into the
harness, with a re-entrancy flag; `regent code` CLI
(`src/regent-cli/src/features/code/cli/codeCommand.ts`) drives the same `code.plan`/`code.start`
RPCs (`regent-deacon/src/application/session_manager/code.rs`).

**Tools** (`regent-tools`): ~30 tools across files/search/terminal/web/memory/skills/board/
persona/vision/camera/computer-use/MCP. `ToolDefinition` = name/description/parameters/toolset
(`regent-kernel/src/contracts/tool.rs`). Catalog dispatch parses args, runs sync before/after
hooks, converts errors to `tool_error_json` (the model always gets well-formed JSON). Deferred
toolsets load on demand (`load_tools`). `ToolContext` carries cwd, an approval handler, and an
optional multi-root filesystem sandbox with symlink-safe containment.

**Skills** (`regent-skills`): full SKILL.md pipeline (agentskills.io subset) — frontmatter
(name, description ≤60 chars, version, created_by, pinned, tags), fs repository, library with
progressive disclosure (level 0 index / level 1 body / level 2 reference files), curator lifecycle
(create/patch/archive; only touches `created_by: agent`), stable-tier prompt index with MRU cap.
**Zero bundled skills ship today, and the harness cannot load any skill.**

**Review** (`regent-agent/src/application/review.rs`): post-turn background fork replays the
unreviewed transcript slice through a whitelisted memory/skills-only catalog — a learning loop,
not a code-review layer. Batch-gated after the review-flood incident.

**Prompts** (`regent-agent/src/domain/prompts.rs`, 408 lines): `SYSTEM_PROMPT` (persona/chat),
`VISUAL_EXPLAINER` (voice), `CONSTITUTIONAL_PROMPT` machinery, `CAPABILITIES` (command surface).
**Nothing coding-specific.** The harness's only coding guidance is two inline turn prompts in
`harness.rs` (`plan_prompt`, `execute_prompt`).

**The headline**: Regent's verify-and-revert gate is ahead of both references — neither opencode
nor Claude Code enforces "tests green or roll back" in the loop. What's missing is everything
*around* the gate: feedback while editing, a fix path when verify fails, dispatch safety, loop
guard rails, a coding voice, and skills the harness can wear.

---

## 2. Gap table

Status: ✅ has it · 🟡 partial · ❌ missing. Priority: P0 = this wave; P3 = later. "CC" = Claude Code.

| # | Capability | opencode | CC | Regent today | Priority |
|---|---|---|---|---|---|
| | **Agent loop robustness** | | | | |
| L1 | Doom-loop detection (repeated identical call → intervene) | ✅ 3-strike ask | 🟡 turn caps | ❌ | **P1** |
| L2 | Budget exhaustion → wrap-up summary, not hard error | ✅ MAX_STEPS_PROMPT | ✅ budget continuations | ❌ `BudgetExhausted` error | **P1** |
| L3 | Mutating tools serialized; parallel only when safe | 🟡 sequential all | ✅ per-tool, input-aware | ❌ ALL parallel (`join_all`) | **P0** |
| L4 | Retry honoring provider `retry-after` | ✅ | ✅ | 🟡 failover chain only | P2 |
| L5 | Reactive recovery from context overflow / output truncation | 🟡 pre-detect | ✅ named transitions | ❌ proactive only | P2 |
| L6 | Interrupt/resume leaves transcript legal | ✅ | ✅ | ✅ settle + repair | done |
| | **Plan / verify / revert flow** | | | | |
| H1 | Read-only plan phase, hard-enforced | 🟡 ruleset | 🟡 mode | ✅ tools physically absent | done |
| H2 | Approval gate before any edit | ✅ | ✅ | ✅ | done |
| H3 | Verify lane + revert-to-green | ❌ | 🟡 skill/hook | ✅ | done |
| H4 | Bounded fix-retry on red verify (failure output → model) | ❌ | 🟡 Stop-hook loop-back | ❌ one-shot revert | **P1** |
| H5 | Edit-time breakage feedback in the tool result | ✅ LSP diags | ✅ LSP diags | ❌ | **P0** |
| H6 | Failing-repro-test-first for bug fixes | ❌ | 🟡 verify skill | ❌ | P2 |
| | **Tool breadth** | | | | |
| T1 | read/write/edit/patch/glob/grep/shell/web | ✅ | ✅ | ✅ | done |
| T2 | Todo / plan-tracking tool | ✅ | ✅ | ❌ | P2 |
| T3 | Explore subagent (isolated context, returns summary) | ✅ task+explore | ✅ | ❌ (board agents are out-of-turn) | **P1** |
| T4 | Ask-the-user structured question tool | ✅ | ✅ | 🟡 approval prompts only | P3 |
| T5 | LSP tool (hover/symbols/references) | ✅ | ✅ | ❌ | P3 |
| T6 | Tool-output truncation with spill-file receipt | ✅ central wrapper | ✅ per-tool budgets | ❌ raw output enters history | **P1** |
| T7 | Validation errors returned as "rewrite your input" feedback | ✅ typed | ✅ typed | 🟡 `tool_error_json` exists, wording ad-hoc | P2 |
| | **Context management** | | | | |
| C1 | Stale tool-result pruning | ✅ 40k-protect | ✅ (2 layers) | ✅ SPL §3.8, cache-aware | done |
| C2 | Summarize with verbatim recent tail | ✅ budgeted tail | ✅ | ✅ protect_last_n | done |
| C3 | Mid-tier collapse of whole tool exchanges | 🟡 | ✅ microcompact | ❌ | P2 |
| C4 | Compaction circuit breaker + pre/post telemetry | ❌ | ✅ | 🟡 telemetry only | P3 |
| | **Review layers** | | | | |
| R1 | Structured post-edit code review | ❌ | ✅ skill/command | ❌ | **P0** (skill) |
| R2 | Security review layer | ❌ | ✅ skill | ❌ | **P0** (skill) |
| R3 | Learning-loop review (memory/skills) | ❌ | 🟡 memory extraction | ✅ background fork | done |
| | **Skill / mode system** | | | | |
| S1 | SKILL.md discovery + progressive disclosure | ✅ | ✅ | ✅ regent-skills | done |
| S2 | Built-in skills shipped in the binary | ✅ 1 | ✅ ~15 | ❌ zero | **P0** |
| S3 | Skills usable by the coding harness | ✅ | ✅ | ❌ chat only | **P0** |
| S4 | Modes / output styles (named prompt swap) | ✅ agents | ✅ styles | ❌ | P2 |
| S5 | Permission rules as data (allow/ask/deny + patterns) | ✅ last-match-wins | ✅ 4 sources + modes | ❌ binary approve/deny | P2 |
| S6 | Deny-with-feedback steers the model | ✅ CorrectedError | ✅ | ❌ | P2 |
| S7 | Lifecycle hooks (user commands at loop seams) | 🟡 JS plugins | ✅ 15+ events | ❌ | P3 |
| | **Prompt quality** | | | | |
| P1 | Coding system prompt: communication, tool discipline, verification habits | ✅ per model | ✅ | ❌ two inline turn prompts | **P0** |
| P2 | Memory conventions in prompt | ❌ | ✅ | 🟡 chat-focused | **P1** (rides P0 port) |
| P3 | Per-model prompt variants | ✅ 7 files | n/a | ❌ | P3 |

---

## 3. The waves

Each wave is independently shippable and leaves `regent code` strictly better. Within a wave,
items are ordered by dependency. Effort marks: S ≈ half a session, M ≈ a session, L ≈ two.

### Wave 1 — feedback and voice (all P0)

The theme: the model finds out it's wrong *while it works*, and it works like a senior engineer.

#### 1a. Safe tool dispatch (gap L3) — M

**Why.** `turn.rs` dispatches every tool call in a batch through `join_all`. Two `file_edit`s on
the same file interleave; an edit and the `terminal` running the build race. Both references treat
parallelism as a per-tool property; Claude Code partitions batches. This is the one latent
*correctness* bug in the loop, so it goes first.

**Design.** Add `read_only: bool` (default `false`) to `ToolDefinition`
(`regent-kernel/src/contracts/tool.rs`). The turn loop partitions each batch into **contiguous
runs** of same-flaggedness, preserving call order: read-only runs execute via `join_all` as today;
mutating runs execute serially, awaiting each. Results re-attach in original call order (the
existing zip already guarantees this given ordered execution).

```text
calls:  [read A] [read B] [edit C] [read D] [edit E]
runs:   [A,B] parallel  →  [C] serial  →  [D] (run of 1)  →  [E] serial
```

Flag `true` for: `read_file`, `glob`, `search_files`, `ls`, `web_search`, `web_fetch`,
`memory_search`, `session_search`, `skills_list`, `skill_view`, `current_time`, `vision_analyze`,
`status`-style regent methods. Everything else stays `false` — flipping a tool to `true` is a
deliberate one-line review, never a default.

**Files.** `regent-kernel/src/contracts/tool.rs` (field); each `definition()` in
`regent-tools/src/infra/*.rs` (one line per read-only tool);
`regent-agent/src/application/agent/turn.rs` (partition, ~30 lines). Check
`regent-providers`' request builders map `ToolDefinition` fields explicitly (they already skip
`toolset`) so the new field never reaches the wire.

**Acceptance.** Unit test in `regent-agent/tests/`: a batch `[read, edit, edit]` with
instrumented executors observes the edits running strictly after each other and after the read
batch; a batch of three reads observes overlap. Existing tests unaffected (single-call batches
behave identically).

**Risk.** Serializing mutating calls lengthens some turns. Acceptable: correctness first, and
mutating batches are rare (models usually edit one file per step).

#### 1b. Edit-time verify feedback (gap H5) — M

**Why.** SWE-agent's headline result and both references' independent choice: breakage reported
in the *same tool result* as the edit prevents error stacking. Regent's verify lane fires once at
the end; a syntax error made in step 3 currently survives until step 30.

**Design.** New `regent-code/src/infra/diagnostics.rs` (~150 lines):

- `detect` reuses `detect_build_tool` (`regent-code/src/domain/mod.rs`) once per harness run.
- `check(workspace, changed_file) -> Option<String>`: runs the cheap per-language check —
  Rust → `cargo check -q --message-format=short`; TS with a tsconfig → `tsc --noEmit`;
  JS → `node --check <file>`; Python → `python -m py_compile <file>`; unknown → skip.
  Hard 10s timeout; on timeout/spawn-failure return `None` and log — **diagnostics must never
  fail an edit**. Output trimmed to first 15 error lines.
- A decorator executor `DiagnosticsWrap` wraps the execute-phase catalog's `file_edit`,
  `apply_patch`, and `write_file` executors: on success, parse the edited path from the args, run
  `check`, and if it reports errors append
  `\n<diagnostics file="src/foo.rs">error[E0308]: …</diagnostics>` to the tool result string.

Wrapping needs one small catalog API: `ToolCatalog::wrap_executor(name, f)` where `f` receives the
original `Arc<dyn ToolExecutor>` and returns a new one
(`regent-tools/src/application/catalog.rs`, ~20 lines). The wrap is applied in
`CodeHarness::execute_phase` only — chat sessions are untouched.

**Acceptance.** Integration test with a temp cargo project: an edit introducing a type error gets
a `<diagnostics>` block in its result; a clean edit gets none; a project with no manifest gets
none. Manual: `regent code "introduce then fix a bug"` shows the model reacting to the block.

**Risks.** Cold `cargo check` can exceed 10s → first check may be silently skipped (fine — the
end-of-run verify still catches it; note it in the tool result? no — silence is fine, log only).
Monorepos where the file's crate isn't the root manifest → `cargo check` at workspace root still
covers it.

#### 1c. Bundled skills + harness skills (gaps S2, S3, R1, R2) — M

**Why & design.** See [§4 Built-in skills](#4-built-in-skills) — mechanism, recommendation, and
the three full SKILL.md drafts.

**Files.** New `regent-skills/skills/{ponytail,code-reviewer,secure-code-guardian}/SKILL.md`
(assets); new `regent-skills/src/infra/bundled.rs` (`include_str!` + parse via existing
`frontmatter.rs`, expose `bundled() -> Vec<SkillRecord>`); `regent-skills/src/application/library.rs`
(merge: disk hit wins, bundled fills the gaps — ~15 lines in `list`/`view`);
`regent-deacon/src/application/session_manager/code.rs` (optional `skill` param on
`code.plan`/`code.start`: resolve body via the library, append to the system prompt handed to
`CodeHarness` — the harness signature doesn't change);
`regent-deacon/src/application/code_task_tool.rs` (optional `skill` parameter, described so the
model picks `ponytail` for "quick and minimal" asks);
`src/regent-cli/src/features/code/cli/codeCommand.ts` (`--skill <name>` flag, pass-through).

**Acceptance.** `regent skills list` shows the three with `created_by: bundled`; a user file named
`ponytail` in the skills dir overrides the bundled one; `regent code --skill ponytail "…"` runs
with the skill body visible in the stored session's system prompt; the curator never
archives/patches a bundled skill (guard on `created_by` already exists — add a test).

#### 1d. System prompt port (gaps P1, P2) — M

**Why & content.** See [§5 System prompt port](#5-system-prompt-port) — the section-by-section
mapping and the full drafted `CODING_PROMPT`.

**Files.** `regent-agent/src/domain/prompts.rs` is 408 lines — over the repo's ~200-line cap
already — so first split it into a module, public API unchanged:

```text
regent-agent/src/domain/prompts/
  mod.rs           — re-exports (pub use), keeps `crate::domain::prompts::*` stable
  system.rs        — SYSTEM_PROMPT, VISUAL_EXPLAINER, CAPABILITIES (moved verbatim)
  constitution.rs  — CONSTITUTIONAL_PROMPT machinery + its tests (moved verbatim)
  coding.rs        — NEW: CODING_PROMPT, WRAP_UP_PROMPT (Wave 2c), EXPLORE_PROMPT (Wave 2e)
```

Then `regent-code/src/application/harness.rs`: both phase agents get
`format!("{CODING_PROMPT}\n\n{surface_system_prompt}")`, and `plan_prompt`/`execute_prompt` shrink
to phase mechanics (their duplicated style guidance moves into `CODING_PROMPT`).

**Acceptance.** Existing prompt tests keep passing after the split (imports unchanged);
new test asserts `CODING_PROMPT` contains the four blocks (communication / tool discipline /
verification / scope) and no Anthropic/Claude identity strings; a stored `regent code` session
shows the composed prompt.

*Wave 1 shippable when:* diagnostics appear in edit results, mutating tools run serially,
`regent skills list` shows three bundled skills the harness can wear, and both phases carry
`CODING_PROMPT`.

### Wave 2 — the harness finishes the job (P1)

#### 2a. Fix-retry on red verify (gap H4) — M

**Why.** A red verify currently throws away everything the failure output could teach. Agentless:
the validate phase is where the wins are; Claude Code's Stop hooks exist precisely to loop back
with feedback.

**Design.** Keep the execute agent **alive** across attempts (its context holds what it just did —
a fresh agent would re-read the world):

```text
execute agent runs plan
loop (attempts 0..=max_fix_attempts, default 2):
    verify
    green | no lane → break
    red & attempts left → agent.run_turn(fix_prompt(outcome.summary))   // same session
    red & none left    → restore snapshot (as today)
```

`fix_prompt` (new, in `harness.rs`): *"Verification failed. Output:\n{summary}\nDiagnose the root
cause and fix it. Do not expand scope; do not disable or delete tests to make them pass."*
`CodeOutcome` gains `fix_attempts: u32`; `max_fix_attempts` rides `AgentConfig`-adjacent harness
config (constructor param, default 2). The snapshot from before execute still guards the whole
sequence.

**Files.** `regent-code/src/application/harness.rs` (restructure `run`, keep the execute agent in
scope across the loop); `regent-deacon/src/application/session_manager/code.rs` (surface
`fix_attempts` in the RPC result); tests: new `regent-code/tests/fix_retry.rs` with mock
`Verifier` scripted red-then-green, asserting one fix turn ran and no revert; red-red-red
asserting revert.

**Risk.** A fix attempt can dig deeper holes — bounded at 2, and the revert backstop is unchanged.
Tests-deleted-to-pass is called out in the prompt and is exactly what Code-Reviewer (1c) flags.

#### 2b. Doom-loop detection (gap L1) — S

**Design.** Local to `run_turn_inner` (doom loops live within a turn): keep the last 2 batch
signatures (`Vec<(name, args_json)>`). If the incoming batch is a single call identical to the
previous two, **skip dispatch** and push a synthetic tool result:
*"You have made this exact call 3 times in a row with identical arguments and identical results.
Change your approach: use a different tool, different arguments, or explain to the user why you
are stuck."* Then continue the loop — the model gets one chance to self-correct; if it repeats
again, the same nudge fires (it converges to budget exhaustion, which Wave 2c makes graceful).
No UI, no permission plumbing — Regent runs headless surfaces (gateway) where opencode's
ask-the-user answer doesn't work.

**Files.** `regent-agent/src/application/agent/turn.rs` (~25 lines); test in
`regent-agent/tests/` with a stub provider scripted to repeat a call 4 times.

#### 2c. Graceful budget exhaustion (gap L2) — S

**Design.** When `max_iterations` or `max_turn_tokens` trips, instead of returning
`Err(BudgetExhausted)`: make **one final** model call with an empty tool list and an appended user
message `WRAP_UP_PROMPT` (*"You have reached this turn's budget. Stop working. Summarize: what you
completed, what remains, and exactly where to resume."*), return its text as `Ok`. The turns
ledger still records `budget_exhausted` (record the outcome before the wrap-up call). One flag
prevents recursion (the wrap-up call itself is exempt from the budget checks).

**Files.** `regent-agent/src/application/agent/turn.rs`;
`regent-agent/src/domain/prompts/coding.rs` (`WRAP_UP_PROMPT`);
`regent-agent/src/application/lifecycle.rs` (outcome recording order). Test: stub provider,
`max_iterations: 2`, assert final text is a summary and ledger says `budget_exhausted`.

#### 2d. Tool-output truncation with receipt (gap T6) — M

**Design.** Central, in `ToolCatalog::dispatch` after a successful execute: if the result exceeds
the cap (default 30,000 chars ≈ 7.5k tokens), write the full result to
`<scratch_dir>/tool-output/<seq>-<tool>.txt` and return the head plus
`\n[truncated — full output at <path>; read_file it only if you need the rest]`. `ToolContext`
gains `scratch_dir: Option<PathBuf>` (set per session by the deacon composition root; `None` =
truncate without spill, keep the head only). Per-tool overrides can come later; one global cap now.

**Files.** `regent-tools/src/domain/entities.rs` (field + accessor),
`regent-tools/src/application/catalog.rs` (~30 lines in `dispatch`),
`regent-deacon/src/application/session_manager/build.rs` (set `scratch_dir`).
Test in `regent-tools`: oversized stub tool → result carries marker + file exists with full bytes.

**Risk.** The spill path must be inside the sandbox roots for jailed sessions — use the session
artifacts area that `allow_subtree` already whitelists.

#### 2e. Explore subagent tool (gap T3) — M

**Design.** New tool `explore` registered for chat and execute-phase catalogs
(`regent-deacon/src/application/explore_tool.rs`, mirroring `background_task_tool.rs`'s pattern):

- Params: `{ question: string, context?: string }`.
- Executes a **fresh** `Agent` with: the session catalog cloned + `restrict_to(plan_toolset(Plan))`
  (read-only by construction — and `explore` itself is not in the allowlist, so no recursion);
  `AgentConfig { max_iterations: 15, max_turn_tokens: Some(60_000), source: "explore", .. }`;
  system prompt `EXPLORE_PROMPT` (*"You are a read-only scout. Answer the question with
  conclusions and exact file paths. Never paste whole files — quote only the lines that matter.
  End with a ≤200-word summary."*).
- Returns the child's final text as the tool result; the child session persists with source
  `explore` (inspectable via `sessions list`).
- Description tells the model when to use it: *"Delegate codebase reconnaissance — 'where is X
  handled', 'how does Y flow' — instead of reading many files into your own context."*

**Files.** `regent-deacon/src/application/explore_tool.rs` (new, ~120 lines);
`regent-deacon/src/application/session_manager/build.rs` (register);
`regent-agent/src/domain/prompts/coding.rs` (`EXPLORE_PROMPT`).
Test: deacon test spawning explore against a fixture repo, asserting the parent transcript grew by
one tool result and the child session exists.

*Wave 2 shippable when:* a scripted red-green verify shows one fix attempt and no revert; a
repeat-call stub trips the doom-loop nudge; budget exhaustion returns a summary; a 100k-char tool
result arrives truncated with a working spill path; `explore` answers without flooding the parent.

### Wave 3 — steering and structure (P2)

**3a. Permission rules as data (gaps S5, S6) — L.** Introduce
`PermissionRule { permission, pattern, action: Allow|Ask|Deny }` with last-match-wins wildcard
evaluation (`regent-tools/src/domain/contracts.rs`), carried on `ToolContext` as
`Arc<[PermissionRule]>`. `ApprovalDecision` gains `DenyWithFeedback(String)`; the dispatch path
returns the feedback as the tool result so the model steers instead of stalling. The existing
binary `ApprovalHandler` becomes the UI for `Ask`. Plan mode keeps its physical catalog restriction
(defense in depth) but the terminal jail and `.env`-read protection become rules. Files:
`regent-tools/src/domain/{contracts,entities}.rs`, call sites in
`regent-deacon/src/application/session_manager/build.rs`, migration notes in the config schema
(name-keyed map discipline per config-schema-shape).

**3b. Todo tool (gap T2) — S.** `todo_write` storing `{content, status}[]` as
`<scratch_dir>/todos.json`, result echoing the rendered list. Registered in chat + execute
catalogs. Files: new `regent-tools/src/infra/todo.rs`, catalog registration in
`session_manager/build.rs`.

**3c. Failing-test-first (gap H6) — M.** Plan prompt gains: *"If this is a bug fix, step 1 of the
plan is a failing test that reproduces it."* Verify gains a fast path: run the named repro test
before the full lane (`regent-code/src/infra/verify.rs` accepts an optional test filter carried
through `CodeOutcome`/plan metadata). Files: `regent-code/src/application/harness.rs`,
`regent-code/src/infra/verify.rs`, `regent-code/src/domain/mod.rs`.

**3d. Mid-tier collapse (gap C3) — M.** Between pruning and compression: collapse whole stale
tool *exchanges* (assistant tool-call + tool result older than the protected tail) into one-line
stubs keeping ids/roles legal — the microcompact idea. Reuses pruning's batching-and-cache-reset
accounting. Files: `regent-agent/src/domain/compression.rs` (+ its tests),
`regent-agent/src/application/lifecycle.rs`.

**3e. Review phases for the harness (rides 1c) — M.** `regent code --review code-reviewer` (or
`security`): after execute, before verify, run a *read-only* phase agent (plan catalog) wearing
the named review skill over `git diff` output; findings append to the final report; `--review`
twice chains both. Files: `regent-code/src/application/harness.rs` (optional phase),
`regent-deacon/src/application/session_manager/code.rs`,
`src/regent-cli/src/features/code/cli/codeCommand.ts`.

### Wave 4 — later (P3)

Lifecycle hooks as user shell commands (pre/post tool-dispatch seams already exist as sync hooks
in `catalog.rs` — the work is config, subprocess protocol, and JSON contracts); an `lsp` tool
(hover/refs; heavyweight — server lifecycle management); per-model prompt variants (worth it only
if telemetry shows instruction-following gaps on non-Claude providers); ask-the-user structured
question tool; `retry-after`-aware backoff in `regent-providers`; compaction circuit breaker
(gap C4).

---

## 4. Built-in skills — summary

Full section (mechanism rationale + the three complete SKILL.md drafts) lives in the companion:
[regent-code-v2-skills-and-prompt.md §4](regent-code-v2-skills-and-prompt.md).

**Recommendation in brief:** bundled SKILL.md files riding the existing `regent-skills` pipeline —
not prompt-injected modes, which would duplicate parsing/listing/viewing/curation and drift.
Bundled via `include_str!` with **disk winning on name collision** (user override by name); the
one new seam is `code.plan`/`code.start` accepting a skill name whose body the deacon appends to
the frozen harness system prompt at session build. The three built-ins: **ponytail**
(YAGNI-ladder implementation), **code-reviewer** (verified, ranked diff findings),
**secure-code-guardian** (trust-boundary map → OWASP sweep → attack-path report; flags anything
widening Regent's own sandbox/jail). Review skills become optional harness phases in Wave 3e.

---

## 5. System prompt port — summary

Full section (section-by-section mapping table, the complete drafted `CODING_PROMPT`, and the
`SYSTEM_PROMPT` refinements) lives in the companion:
[regent-code-v2-skills-and-prompt.md §5](regent-code-v2-skills-and-prompt.md).

**In brief:** from the Fable-5 consumer prompts, port `tone_and_formatting`/`lists_and_bullets`
(→ communication block), the mistakes-handling line, the check-don't-assume seed (→ verification
block), the `str_replace`/`create_file`/`bash_tool` guidance (→ tool-discipline block), the
search-scaling rule, and the memory application conventions (→ shared `SYSTEM_PROMPT`). Do NOT
port Anthropic/Claude identity, refusal/wellbeing/child-safety (constitution territory, ADR-028),
or claude.ai harness tools (artifacts, connectors, image search…). Target: new
`regent-agent/src/domain/prompts/coding.rs` (`CODING_PROMPT`, four blocks: communication · tool
discipline · verification · scope), after splitting the 408-line `prompts.rs` into a `prompts/`
module. Regent's identity, persona layering, and tool names stay.

---

## 6. Testing strategy

- **Wave 1a**: `regent-agent/tests/dispatch_order.rs` — instrumented executors assert
  parallel-reads / serial-writes / original-order results.
- **Wave 1b**: `regent-code/tests/diagnostics.rs` — temp cargo fixture; broken edit → block,
  clean edit → none, no-manifest → none, slow check → skipped not failed.
- **Wave 1c**: `regent-skills` unit tests — bundled present, disk overrides by name, curator
  refuses `created_by: bundled`; deacon test: `code.plan` with `skill: "ponytail"` stores a system
  prompt containing the ladder.
- **Wave 1d**: prompt tests — split preserves public API; `CODING_PROMPT` contains the four
  blocks; no "Claude"/"Anthropic" strings.
- **Wave 2**: `regent-code/tests/fix_retry.rs` (scripted verifier red→green: one fix turn, no
  revert; red×3: revert); `regent-agent/tests/doom_loop.rs`; budget wrap-up test (stub provider,
  `max_iterations: 2`); truncation spill test in `regent-tools`.
- **Manual, each wave**: one real `regent code` run on this repo with the outcome pasted into the
  PR description — the harness's own dogfood is the acceptance bar.

## 7. Sequencing and dependencies

Wave-1 items are mutually independent; 2a wants 1b first (diagnostics reduce red-verify frequency
before the retry loop matters), 2e wants 1a (its child inherits dispatch safety), 3e depends on
1c's harness-skill seam; everything else is order-free. Suggested first implementation session:
**1a + 1d** (both small, both unblock everything, and the prompt split de-risks the file-cap
violation that already exists). Second session: 1b. Third: 1c.

## 8. Done means

Wave 1 merged: diagnostics in edit results, serial mutating dispatch, three bundled skills the
harness can wear by name, `CODING_PROMPT` live in both phases, prompts.rs split under the file
cap. Waves 2–4 each stand alone; re-run the gap table after each wave and re-prioritize — 2e
(explore) in particular may jump to P0 in practice once diagnostics lengthen execute-phase
transcripts.

## 9. Reference — file inventory (jargon corner)

**Current:** `regent-agent` — `application/agent/{mod,turn}.rs` (loop),
`application/{lifecycle,review}.rs` (ledger/compress-split, learning fork),
`domain/compression.rs` (prune + summarize), `domain/prompts.rs` (408 lines, to be split),
`domain/config.rs` (`AgentConfig`: max_iterations 90, ctx 128k, compression{0.5, protect 20,
prune-after 5}). `regent-code` — `application/harness.rs` (plan/approve/execute/verify/revert),
`domain/mod.rs` (`Phase`, `plan_toolset` 11 read-only tools, `detect_build_tool`,
`parse_verify`), `infra/{checkpoint,verify}.rs`. `regent-skills` —
`application/{library,curator,prompts}.rs`, `domain/{entities,contracts,errors}.rs`,
`infra/{frontmatter,fs_repository}.rs`. `regent-tools` — `application/catalog.rs`
(dispatch/hooks/restrict_to/deferred), `domain/{entities,contracts}.rs` (`ToolContext`, sandbox,
`ApprovalHandler`), `infra/*` (~30 tools). `regent-kernel` — `contracts/tool.rs`
(`ToolDefinition`), transcript/session types. `regent-deacon` —
`application/code_task_tool.rs`, `application/session_manager/{build,code}.rs` (composition root,
code RPCs). CLI: `src/regent-cli/src/features/code/cli/codeCommand.ts`.

**New files proposed:** `regent-code/src/infra/diagnostics.rs` ·
`regent-skills/src/infra/bundled.rs` + `regent-skills/skills/{ponytail,code-reviewer,secure-code-guardian}/SKILL.md` ·
`regent-agent/src/domain/prompts/{mod,system,constitution,coding}.rs` (split) ·
`regent-deacon/src/application/explore_tool.rs` · `regent-tools/src/infra/todo.rs` (Wave 3) ·
tests: `regent-agent/tests/{dispatch_order,doom_loop}.rs`,
`regent-code/tests/{diagnostics,fix_retry}.rs`.
