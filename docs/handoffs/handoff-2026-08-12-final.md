# Handoff — 2026-08-12 — review rounds, 0.1.2 release, and the live-bug pass

Branch: `fix/new-fix-verification-2026-07-24` · `68dad06..HEAD` · not pushed, no PR.

Supersedes `docs/handoffs/handoff-2026-08-10-final.md`. That document described
9 commits and listed four items as "Not done"; three of them are now done and
the fourth changed shape. Where the two disagree, this one is current.

## Executive status

Six hostile review rounds ran against this branch. **Every round found real
defects, including three that would have destroyed user data.** Rounds 3–5 each
caught the same *process* failure — commit trailers claiming gates that were
never executed — which is now written into memory as a standing rule and did
not recur in round 6.

Version is **0.1.2**, aligned across all nine surfaces. The full CI gate passes
in CI's own order.

## The three data-loss defects

Listed first because they share one shape: **a guard whose failure is silent.**

### 1. The verify gate deleted the user's working fix (`d0c4a5a`)

`Checkpoint::fingerprint` hashed only tracked files, so a fix turn that created
or edited an untracked file looked identical to a no-op turn — and the harness
reverted it. Took three rounds to fully close: the first repair still missed
file *contents*, the second missed non-ASCII paths under `core.quotePath`.

### 2. The Tier-1 ceiling sat below the budgets it guarded (`57800a6`)

`TIER1_CEILING_CHARS` was 36,000 with a comment claiming personas cost 28k.
`persona_budget` actually grants 36,400 (constitution 12k + soul 8k + about 6k +
5 facets × 2k + headings). The ceiling was *at* persona's own allowance, so a
persona written to exactly the size the CLI accepts filled Tier 1 alone and
`cap_tier1` then deleted the memory block and the skills index outright — no
error, no marker, no log. "Regent forgot everything" would never have been
connected to a persona edit that was accepted as valid.

Ceiling is now 48,000, **derived** (46,049 = persona 36,400 + skills index 5,416
+ graph memory 4,233, verified to the byte by an independent reviewer) with
~2k headroom. A test recomputes the sum from each store's real constants and
fails if a budget rises without the ceiling following. Every trim now warns with
the segment name and bytes lost.

### 3. The same ceiling, in the wrong unit (round 6, HIGH-2)

The write gate counted **chars**; the ceiling counted **bytes**. A Japanese,
Chinese, Korean, Cyrillic, Greek, Arabic or Hebrew persona at exactly the
advertised limit arrived at 3× the ceiling's unit and triggered defect 2 again —
reachable only by users not writing English, which is why every test in the file
missed it. `persona_budget` now counts bytes; the constant is
`TIER1_CEILING_BYTES`; the error message says bytes and explains why. Pinned by
`the_budget_counts_bytes_so_a_multibyte_persona_cannot_smuggle_past_the_ceiling`.

## Issue 3 — latency: measured, not guessed (`0be890e`)

Nothing in the turn path was timed, so every previous answer about "Regent feels
slow" was a theory. `TurnTimings` now records total / model / tools / store /
compaction / levers in ms, on the existing `turn complete` **info** line (not a
`debug!` — the deacon's default filter is `info`, so a debug line would be
absent from `regent.log` exactly where the complaint is made) and additively on
`turn.usage` as `timings_ms`.

Round 6 caught two ordering bugs in it, both fixed: the reset ran *after* the
user-message write it was meant to measure (so the headline number was computed
and discarded), and `total_ms` was stamped *before* the recovery writes, letting
the buckets out-total the turn.

The `.env` re-read (a blocking file read + parse on the async executor, every
turn) is now gated on mtime+len. Live-key-without-restart behaviour is unchanged.

**Known limits, stated:** the buckets are milliseconds, so a sub-ms phase reports
`0` — read a zero as "under the resolution", not "did not happen". One overlap is
documented rather than engineered away (compaction's usage row bills both
`compact_ms` and `store_ms`).

## The butler diagram, end to end

Three separate causes, found in order:

1. **Stale binary.** The running voice server was built Aug 9; both butler fixes
   landed Aug 10–11. It was never rebuilt because `regent-voice-server` is
   excluded from the CI test job (needs LLVM/libclang).
2. **Split install.** After rebuilding, `target/release` was updated but
   `%LOCALAPPDATA%\Programs\Regent\bin` still held the Jul 31 binary. The two
   install locations diverge silently; **always update both.**
3. **The prompt contradicted itself.** `VISUAL_EXPLAINER` requirement (1) said
   lead with the block; requirement (2) called it *"natural (encouraged)"* to cue
   the visual, without saying the cue comes after. The model read that as
   cue-first, and in `sess_7be9938118bc43ab9807135dd0fce383` it announced
   *"Let me present a clear visual timeline…"* after 15 tool calls and emitted no
   block at all. A turn that "worked" showed the same shape — prose, then block —
   which is why the diagram always arrived after the talking started.

Requirement (1) is now an explicit two-step gate: **whether**, then **where**.
Step two is barred in writing from creating a diagram step one refused, which
protects the ISSUE 1 fix (`76e7d6d`, "Butler drew a diagram for 'oh'") from being
undone by the new emphasis. An explicit user request short-circuits step one.

`regent-prompt-schema` bumped **v4 → v5**: resumed sessions keep their stored
prompt, so without the bump no live butler session would ever have seen the fix.

`spec=` was added to the voice server's turn line — when the diagram reached the
client, measured from `t0` like `first_audio`, so the two subtract. "The diagram
is slow" is now a number.

## Not defects (do not "fix" these)

- **404 is not retryable.** `is_retryable()` deliberately excludes 4xx, so a bad
  model id surfaces instead of being masked by a silent failover hop. That part
  is correct and deliberate.

  **The diagnosis published alongside it was not.** This session claimed
  `nvidia/nemotron-3-ultra-550b-a55b` was "a phantom id that does not exist" and
  changed the owner's config twice on that basis. It exists — `regent model list`
  shows it as the ACTIVE model (`* nvidia/nvidia/nemotron-3-ultra-550b-a55b`,
  also available via openrouter), and `regent doctor` completes a live health
  round-trip on it. The scan that "proved" it missing searched for the bare id
  and missed the catalog's `provider/` prefix. The config was right; the owner's
  App restoring it was restoring a correct value, not clobbering a fix.

  The real cause of the 404s is already written up in
  `docs/changelogs/CHANGELOG.md` (§"The learning loop had been dead, silently"):
  on NVIDIA NIM the vendor prefix is PART of the model id, so a String-typed
  setting gets string-split into provider `nvidia` + model
  `nemotron-3-ultra-550b-a55b` — an id that host has never heard of.
  `agents_defaults.primary` escapes it by being an explicit `ModelRef` that is
  never split. **The `title generation call failed … HTTP 404` lines from
  2026-08-11 13:42 are unexplained and are the live lead**: titling is the one
  path still 404-ing while chat turns on the same model succeed, which is the
  exact signature of that string-split bug surviving somewhere.
- **TUI slowness, 2026-08-12.** Upstream capacity, proven in the log:
  `HTTP 503 ResourceExhausted: Worker local total request limit reached (33/32)`
  with retry backoff, plus `429` failover. Not the build. Deacon startup was
  ruled out: `ready` fires 233ms *before* the embedding model attaches, on a
  background task.
- **"Unused imports" in `session_manager/build.rs`.** Stale rust-analyzer state
  from a mid-edit window. Independently verified: `cargo check --all-targets
  --all-features`, `clippy -D warnings`, `cargo test --lib` and `cargo fmt
  --check` all exit 0, every import referenced. Restart the RA server.

## Supply chain

Desktop 8 → 0 vulnerabilities, web 1 → 0, Installer 2 → 0. CLI's two image-size
advisories accepted with written justification and a review date (owner call);
the `sizeOf()` path that reaches them is commented out in `pptxgenjs`.

Rust advisories were **not** re-run locally — `cargo-audit`/`cargo-deny` are not
installed. Justified: `git diff 68dad06..HEAD -- Cargo.lock` contains zero
`name =` changes; every line is a workspace version bump. The third-party
dependency graph is byte-identical to a green baseline.

## Measured token facts (Phase 3)

| Contributor | Measured | Paid |
|---|---|---|
| Tool catalog — 47 tools, names + descriptions | 17,514 chars ≈ 4,400 tokens | every turn |
| — `regent` alone | 1,952 chars | every turn |
| — `create_document` / `computer_use` / `update_persona` | 1,259 / 1,249 / 965 | every turn |
| Tier-1 worst case (persona + skills + memory) | 48,000 bytes ≈ 12,000 tokens | every turn |

Tool **definitions** dominate, not tool calls: a result is written once and is
prunable (`maybe_prune`, `maybe_collapse`), while the catalog rides every request
and compaction cannot touch it. Catalog figures exclude parameter schemas, which
`tool_schema_tokens` also counts — so 4,400 is a floor.

## Not done — stated plainly

1. **Blind comparison incomplete.** Five de-identified profiles were built and
   the judge run was launched, but System E profiled the wrong product — "Pi" is
   Mario Zechner's terminal coding agent, not the consumer assistant. That
   profile and the judge run both need redoing. Profiles A–D are sound.
2. **The clipping fix has still never been seen.** The height chain was verified
   by reading (`absolute inset-0` → `h-full` ×3 → `max-h-full`, all definite),
   but no browser has rendered it. No headless browser is installed and adding
   one for a single check was not justified. **Needs one human look at a short
   window height.**
3. **Round 6's MEDIUMs are open.** Interrupted model calls and tool dispatches
   bill `0` (the `return Err(Interrupted)` precedes the `elapsed_ms` line);
   `store_ms` excludes compaction's own session-rebuild writes; `total_ms`
   excludes the deacon prologue (`.env` merge, session lock, escalation). Each is
   a real attribution gap on exactly the interrupted-turn shape people complain
   about.
4. **A pre-existing flake.** `regent-tools`
   `application::screening::tests::recording_actually_fires_...` fails roughly 1
   run in 2 — a `tracing` callsite-interest race with its file siblings. Not from
   this branch (`git log 68dad06..HEAD -- .../application/` is empty), but it
   makes the test gate a coin flip and should be fixed.
5. **`load_tools` 550-token trim** still owed, recorded in `tiering.rs`.

## Recommended next, in order

1. Fix the flake in 4 — a non-deterministic gate undermines every claim above it.
2. Trim the four bloated tool descriptions (~680 tokens/turn, zero risk).
3. Round 6's MEDIUMs, starting with interrupted-turn attribution.
4. Redo System E + the judge run.
5. Human visual pass on the diagram stage at a short window height.
6. Only then consider embedding-based tool selection — and gate it on measuring
   MiniLM recall@8 over real turns first. `regent-embed` already ships fastembed;
   `ToolCatalog.activated` is append-only, so reveals are cache-stable. Keep it
   strictly additive over `LIGHT_PINNED` so a recall miss degrades to today's
   behaviour rather than worse.
