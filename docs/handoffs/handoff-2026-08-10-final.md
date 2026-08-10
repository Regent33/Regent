# Handoff — 2026-08-10 — pre-handoff investigation, fixes, and Prime Agent study

Branch: `fix/new-fix-verification-2026-07-24` · Commits `68dad06..894db0d` (9)
· Not pushed, no PR opened.

This handoff supersedes the status sections of
`docs/handoffs/handoff-2026-08-09-regent-refining.md`. Where the two disagree,
this one is current — in particular, the three items that handoff listed as
open are addressed below.

## Executive status

Six reported issues were investigated against source before any code changed.
**Four were real defects and are fixed. Two were not defects in the source at
all** and are recorded as such rather than papered over with a change.

The work is evidence-based: every fix ships a test that fails without it, and
the before/after counts are in the commit messages. Two items in the brief
remain genuinely incomplete and are listed under "Not done" — they are not
hidden.

## Bugs fixed, with root causes

### 1. Two deacons reviewed the same conversation twice (`b8ed4f0`)

**Root cause.** The learning loop's batch gate was `review_gate`, an in-memory
mutex. It fences one process. Regent routinely runs two deacons against one home
— the CLI spawns one per command beside the voice server's long-lived one — so
live logs showed one parent session reviewed at 22:44 and again at 22:46: two
model calls, two bills, one range.

**Fix.** Ownership moved to the shared store. A review takes a token-fenced
claim inside one `BEGIN IMMEDIATE` transaction *before* the reviewer session or
any model call exists, renews it every 100 s while working, and commits by
advancing the reviewed cursor and clearing ownership in a single statement.
`Busy` stops the second process before it spends anything; `Covered` makes a
late arrival adopt the durable cursor. A crashed owner cannot release, so the
300 s lease — not the release call — bounds the stall, and the token prevents a
stale owner committing over the successor that reclaimed its expired lease.

**Migration.** Four nullable reconciled `sessions` columns. `SCHEMA_VERSION`
unchanged at 11; all-NULL reads as "unclaimed", which is exactly how every
pre-existing row already behaves. No numbered migration.

### 2. Butler drew a diagram for "oh", and none for a real request (`76e7d6d`)

**Root cause — both halves were client-side.** The model was instructed
correctly all along (`system.rs` already says "DO NOT emit one for greetings").
- False positive: the last-resort generator returned a spec for any reply with
  ≥1 explanation point, and when it had only one it *fabricated* a second node
  labelled `Result`. Its only suppressor was a fixed pleasantry word list
  containing no backchannels — which is why each field report added a word and
  the next one slipped through.
- False negative: `isSmallTalk` was `^`-anchored, so a greeting *prefix*
  condemned the whole turn. "hey, explain the history of Rome" was small talk,
  which vetoed the model's own spec, skipped the fallback, and suppressed the
  filler cue.

**Fix.** The bar is now the shape of the turn, not a word list: a one-word turn
with no question earns no automatic visual, one point is not an explanation, and
the fabricated node is deleted. The small-talk fix went into the **shared**
predicate, so all three callers recovered together.

### 3. "Pull up the last website" — no record of a URL that was in the DB (`707fb89`)

**Root cause.** Two independent things.
- Every read path over the action log was relevance-ranked (`search_messages`,
  `ORDER BY rank`) or metadata-only (`session_list` returns titles). Nothing
  answered "what did I most recently *do*".
- The premise "this current session" was false: the Butler voice surface owns
  its own session rows, so a site opened by voice is genuinely not in the chat
  transcript.

**Fix.** `session_list` gained an `actions` mode over a new
`Store::recent_actions`, returning newest-first tool calls with results and
originating arguments, **across surfaces** — a per-session lookup would
reproduce the bug. Results capped at 300 chars so a recall tool cannot become
the context problem it was added to solve.

### 4. Diagram slow and clipped; spec read aloud (`7dbdcae`)

Four independent causes:
- **Clipping was not a mistimed fit — there is no fit step.** The stage spends
  368 px on gutters, then capped the SVG at a fixed `58vh`, i.e. against the
  *window* rather than the box the padding left. Any window under ~876 px
  clipped. Now `h-full` down to the host with `max-h-full`.
- **Cold start**: the mermaid chunk was imported on the first diagram, inside
  the audio gate. `warmMermaid()` on mount overlaps it with ASR and first token.
- **Gate hold**: released on the tween's `onComplete`, so every diagram turn
  held narration for the whole decorative stagger. Now `onStart` + the existing
  double-rAF.
- **Bare spec**: a weak model drops the ``` and emits the spec as a bare leading
  object; `FenceGate` only understood fences, so the JSON was spoken *and*
  denied the early flush. One guard in the shared `push`, with string/escape
  tracking and a 4096-char bound so a stray brace cannot mute a turn.

### 5. Background job completion could not be replayed (`2b6ce84`)

**The reported symptom does not reproduce.** A push (`job.finished`) and a
durable pull (undelivered outcomes prepended to the next real turn, marked
delivered only after that turn succeeds) both already existed, from `f0c7b4b`.
Nothing was rebuilt.

**The real defect**: the push is best-effort stdio rendered into client-local
state, and `job.list` returned only queued/running rows — so a reload, route
change, or restart lost the notice. Fixed at the shared seam:
`JobLedger::undelivered()` exposes the store's existing "terminal AND
`delivered_at IS NULL`" set, and `job.list` returns it with a `delivered` flag.
Every client already speaks `job.list`. Desktop replays on mount (deduped by job
id); the CLI got its first `job.finished` case at all; the tool's prompt was
narrowed from "I'll report back" to what the architecture actually guarantees.

### 6. Verify gate re-run over an untouched tree (`894db0d`)

Adopted from the Prime study. `regent-code`'s fix loop called the verifier after
every fix turn, including turns that edited nothing — spending the whole
build/test lane to be told the identical failure. Now fingerprinted before and
after; the attempt is still consumed so a stalled model cannot spin for free.
**Measured: 3 gate runs → 1.**

## Not defects (do not "fix" these)

- **API Keys → Vision.** Already implemented in source: `visionHeading`
  ('Vision & video analysis') exists and the deacon groups the key as `vision`,
  proven by `media_key_rows_match_the_adapters_regent_actually_ships` (9/9
  passing). The old layout is visible because a **User `REGENT_DEACON_PATH`
  variable pins the installed deacon to a pre-fix build**. This is a deploy
  step, not a code change.
- **Background-task reporting**, per §5 above.

## Deploy steps the owner must take

1. **Swap the installed deacon.** Close the Desktop app (it holds the exe), copy
   `target/release/regent-deacon.exe` over
   `%LOCALAPPDATA%\Programs\Regent\bin\regent-deacon.exe`, relaunch. Or clear
   the `REGENT_DEACON_PATH` user variable so the newest repo build wins.
2. **Rebuild the Tauri app.** The Butler diagram fixes and the job-notice replay
   are frontend changes and are not live until then.

## Token / context findings (Phase 3)

Measured, not estimated (bytes ÷ 4, the convention `telemetry.rs:46` uses):

| Contributor | ~tokens | Verdict |
|---|---|---|
| persona `constitution` core | 2,302 | must stay — already well designed (6 of 16 sections inline, rest via memory_search) |
| persona `about.*` facets + soul | 1,802 | **the one unbounded contributor** — needs a per-facet ceiling |
| SYSTEM_PROMPT | 1,654 | must stay |
| CAPABILITIES | 1,269 | partial overlap with SYSTEM_PROMPT; audit |
| resident tool schemas | ~2,000 | each earned, documented |
| `load_tools` deferred index | ~900 | **cheapest real win** — 60-char hook × ~45 tools |

The model-facing tool catalogue was found sitting at **exactly its 3,150-token
ceiling with zero headroom**. The `actions` mode cost +19 and the ceiling was
raised to 3,175 deliberately, with `load_tools` (550 tokens, 17 % of the
catalogue) recorded in the gate's own comment as the repayment target so the
ceiling comes back down rather than drifting up.

`cap_tier1` trims from the END and persona renders FIRST, so persona is trimmed
last — which is why the unbounded facet text is the right target, not the
engineered prompts.

## Prime Agent study

`docs/research/prime-agent-architecture-study.md` (that directory is gitignored
by owner decision, so the file is local-only and not in the commits).

Prime's agent has **exactly one tool** (`ToolName = "ipython"`); every capability
is Python in a persistent kernel calling back into TypeScript. Fixed per-turn
cost ~2.4–3.2 K tokens vs Regent's measured 8.1 K. It has **no permission
system, no sandboxing, no iteration cap, no token ceiling, no doom-loop
detection, and no retrieval of any kind**.

Adopted: unchanged-worktree gate (shipped), structured compaction template
(shipped). Recommended next: positive completion evidence (never treat "the
assistant stopped talking" as done), progress snapshots for background jobs, a
versioned deacon wire protocol. Rejected with reasons: the CodeAct core itself
(not portable to a broad typed tool surface), claim-before-deliver dispatch
(already solved by `regent-cron`'s tick lock), a merged JSONL sink (`tracing`
spans are better), capability-docs-by-reference (the deferred-tool index already
is this).

## Tests run

| Suite | Result |
|---|---|
| `cargo test -p regent-store` | 74 passed |
| `cargo test -p regent-agent` | 105 passed |
| `cargo test -p regent-tools` | 331 passed |
| `cargo test -p regent-deacon` | 341 passed |
| `cargo test -p regent-jobs` | 36 passed |
| `cargo test -p regent-code` | 26 passed |
| `cargo test -p regent-voice-server` | 60 passed |
| Desktop `bun test` / `typecheck` | 330 passed / clean |
| CLI `bun test` | 205 passed |
| `cargo clippy` (all touched crates) | clean |

Failing-before proofs: review lease `left: 2, right: 1` → green; diagram
predicates 7 failed → 31 passed; fence 3 failed → 9 passed; verify gate 3 runs
→ 1.

## Not done — stated plainly

1. **No independent adversarial review ran.** Three attempts to fan out a
   hostile critic died on account session limits (8 of 10 agents, then 5, then
   3). The fixes were implemented and self-verified with failing-before proofs,
   but **no separate reviewer has judged this work**, and the blind
   side-by-side ranking against Hermes / Claude Code / Pi / Prime Intellect was
   not produced. This is the largest outstanding item.
2. **Issue 3 (latency) is measured but unfixed.** Confirmed real: a blocking
   `.env` read executed inline in an async fn every turn; two synchronous SQLite
   reads on the dispatcher task before the turn spawns; the user message
   persisted and awaited before the first model call; compaction inserting a
   full extra model round-trip. **Nothing in the turn path is timed**, so no
   stage can be attributed — spans must land before this can be closed honestly.
   Five popular theories were disproven with code evidence: catalog rebuilt per
   turn, embedding warmup on the hot path, memory retrieval blocking, title
   generation blocking, redundant model loops. None are real.
3. **The clipping fix was never seen.** It is reasoned from the layout chain and
   typechecks, but needs one visual pass at a short window height.
4. **Persona facet ceiling not implemented** — the largest single token
   contributor is still unbounded.

## Known debt

- `recent_actions` does one extra query per returned row to fetch originating
  arguments (≤25 rows). Fine for a user-triggered recall; not for a hot path.
- Wire vocabulary mismatch: `job.finished` emits "finished" even where the
  ledger recorded `Inconclusive`. Cosmetic, separate.
- `regent-gateway` registers neither the background-task tool nor `wrap_prompt`
  — consistent with the known gateway parity drift.
- Blocking SQLite calls inside async review tasks are not `spawn_blocking`-wrapped
  (pre-existing pattern, not introduced here).

## Recommended next steps, in order

1. Run the adversarial review + blind comparison once account limits reset.
2. Add turn-path tracing spans, then act on the three confirmed serial costs.
3. Bound the persona facets; trim the `load_tools` hook and lower the catalogue
   ceiling back below 3,150.
4. Visual pass on the Butler diagram stage; then the two deploy steps above.
