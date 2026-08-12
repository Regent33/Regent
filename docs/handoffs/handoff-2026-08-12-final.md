# Handoff — 2026-08-12 — review rounds, 0.1.2 release, and the live-bug pass

Branch: `fix/new-fix-verification-2026-07-24`, fast-forwarded into `main` and
pushed (`c6a673f..1a279a9`, 348 commits). No force, no history rewritten.

Supersedes `docs/handoffs/handoff-2026-08-10-final.md`. That document described
9 commits and listed four items as "Not done"; three of them are now done and
the fourth changed shape. Where the two disagree, this one is current.

## Executive status

Eight hostile review rounds ran against this branch. **Every round found real
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
+ graph memory 4,233 — reproduced independently, though round 8 showed the
skills term was 140 CHARS presented as bytes and the entry separator is
unmodelled, so treat it as an internally consistent estimate, not a bound) with
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
  never split.

  **Found and fixed in `5d79fe8`.** The `title generation call failed … HTTP 404`
  lines from 2026-08-11 13:42 were that same bug reached by a second route:
  titling resolves through `SessionManager::provider()`, which passes
  `current_model` as a **String** into `resolve_model_str`, where the prefix
  split ran before the explicit-`models:` rung. The guard added in the
  2026-08-06 pass only covers the case where the DEFAULT is already this exact
  id, and here the default was a different provider's model entirely. An
  explicit `models:` listing now outranks the split. **Round 7 flagged that this
  reorder is broader than the bug required** — see "Not done" — so treat it as
  fixed-with-a-caveat, not settled.
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
| Tier-1 derived worst case (persona + skills + memory) | 46,049 bytes ≈ 11,500 tokens | every turn |
| Tier-1 ceiling (the cap, not the expected size) | 48,000 bytes | — |

Tool **definitions** dominate, not tool calls: a result is written once and is
prunable (`maybe_prune`, `maybe_collapse`), while the catalog rides every request
and compaction cannot touch it. Catalog figures exclude parameter schemas, which
`tool_schema_tokens` also counts — so 4,400 is a floor.

## Verified in the real app (2026-08-12)

- **Butler mode works**, on the v5 prompt and the rebuilt voice server — owner
  confirmed. The prompt fix, the schema bump and the voice-server rebuild are
  no longer inferred from source; they were run.
- **The diagram stage was seen and is correct** — owner confirmed. The clipping
  fix had been reasoned from the layout chain (`absolute inset-0` → `h-full` ×3
  → `max-h-full`, all definite) and never rendered; it has now been looked at.
  That closes the VISUAL half of "visually and programmatically", which had
  been open since the fix shipped.
- Still unmeasured: `spec=` has read `None` on every turn logged so far, so the
  EARLY-FLUSH path (diagram on screen before speech begins) is unconfirmed. The
  diagram draws; whether it beats the voice is a number nobody has yet.

## Round 7 verdict: DO_NOT_SHIP — open blockers

Reproduced CI exactly (fmt pass, clippy 0, 1301 passed) and **disproved round 6's
flake**: 10 clean observations, so a 50% rate has p≈0.001. It was mischaracterised,
not fixed. Then it found this, all file:line-verified:

1. **HIGH-A — the timing test cannot fail for the bug it names.**
   `regent-agent/tests/agent_loop/turn_flow.rs` asserts only `summed <= total_ms`.
   Discarding a bucket's time makes `summed` SMALLER, so the bound is
   monotonically insensitive to the defect; on that fixture every bucket floors
   to 0, making it `0 <= total_ms` — true for all inputs. The `store_ms > 0`
   assertion removed after it failed was the only one that could fail correctly.
   A resolution-independent version (wrap `Store`, assert billed-write count ==
   persisted-message count) costs the same lines.
2. **HIGH-B — "units agree" is false.** `regent-graph` still counts CHARS
   (`application/entries.rs:36`, `domain/policy.rs:21`, budgets 2,200 + 1,375)
   and `tests/prompt_lines.rs` sums those char limits with a byte length against
   `TIER1_CEILING_BYTES`. A CJK memory graph at budget contributes up to 10,725
   bytes where the arithmetic budgets 3,575 — 7,150 over, against 1,951 of
   headroom. `cap_tier1` truncates memory and can empty the skills index. **The
   same data-loss bug this branch claims twice to have closed, one crate over.**
3. **M-5 — the routing reorder is broader than the bug required.** With a direct
   `anthropic` provider AND an `openrouter` provider listing
   `anthropic/claude-opus-4-8` (the shape of this repo's own fixtures), that spec
   now routes to OpenRouter — a silent vendor/key/billing switch on upgrade. The
   NIM bug only needed listing to win when it names the SAME provider the prefix
   names. Untested collision.
4. **M-1** the distiller compares chars to the byte budget, so CJK personas can
   never auto-consolidate; **M-2** no migration for rows already over budget;
   **M-3** the `total_ms` move is unobservable (emitted only on `is_ok`, the
   recovery writes only on `is_err`); **M-4** interrupted turns still bill
   `model_ms`/`tools_ms` = 0, unmentioned in a commit that discusses interrupted
   turns.
5. **LOW** — VISUAL_EXPLAINER still says "Prefer emitting a block over skipping
   when a topic is at all explanatory", which points the opposite way to STEP
   ONE's "Most turns have not"; the new assertions are `contains()` checks on
   phrases just added, so they pin wording against deletion but cannot detect a
   contradiction elsewhere in the same constant. Prompt cost +376 tokens/turn
   with one clause duplicated verbatim. `constitution.rs:97` is a latent repeat
   of the unit bug (harmless today by slack).

Cleared on inspection: the reset does precede the first store write on every
path; the v5 bump genuinely reaches resumed sessions; every persona writer goes
through the byte gate; the phantom-id retraction is complete.

Scores: quality 5, efficiency 7, coding 6, self-learning 4, reliability 6,
agentic 6.

## Not done — stated plainly

The single list of open items. Anything not here is closed; anything here is not.

1. **Round 8's leftovers.** Its two HIGHs and two MEDIUMs are FIXED (skills index
   → bytes, the slack second assertion → `< before`, the three agent-facing
   "chars" messages, the over-budget shrinking replace). Still open: the entry
   separator (4 bytes per gap, no entry-count cap) is unmodelled in the ceiling
   derivation, and `provider_registry.rs:216-218` overclaims — on a provider with
   no `models:` key, a bare `org/model` spec still falls through to the primary,
   which the comment says it prevents.
2. **M-2, the data half.** Persona and graph rows written under the old CHAR
   budget can exceed the BYTE budget with no new write. Reads are unaffected,
   nothing is deleted, and the code half is now fixed — a shrinking replace is
   accepted while over budget, so a store can be consolidated back under. But
   there is no migration and no boot warning, so nobody is told. **Decide:
   migrate on boot, warn once, or state it in the release notes.**
3. **Interrupted turns bill zero.** `return Err(Interrupted)` precedes the
   `elapsed_ms` line in `turn/model_call.rs` and `turn/dispatch.rs`, so a
   cancelled 30s model call reports `model_ms=0`. `store_ms` also excludes
   compaction's session rebuild and the turns-ledger row; `total_ms` excludes the
   deacon prologue. Attribution gaps on exactly the turn shape people complain
   about.
4. **A latent unit repeat.** `prompts/constitution.rs:43` packs chunks by CHARS
   (`CHUNK_CHARS = 1_800`) against graph memory's now-BYTE 2,000 cap. Measured
   safe today (18 chunks, max 1,784 bytes), and `application/constitution.rs:54`
   only warns on rejection — so a doc edit adding multi-byte punctuation could
   silently drop an always-on constitutional section. `constitution.rs:97`
   compares chars to the byte budget for the same reason.
5. **`load_tools` 550-token trim** still owed, recorded in `tiering.rs`.
6. **Chat hoists the LAST diagram.** With two spec blocks in one reply,
   `extractPresentSpec` scans last-first, so the second renders above the prose
   and the first renders in place — each drawn once, order inverted.

**Not open, contrary to earlier drafts of this document:** the `regent-tools`
screening flake. Round 7 disproved it with 10 clean observations and round 8
added 7 more (p≈0.001 against a 50% rate). It was mischaracterised, never real.

## Recommended next, in order

1. Decide M-2 (item 2) — the only item that changes behaviour for an existing
   user on upgrade, and therefore the only one that gates a release.
2. Interrupted-turn attribution (item 3) — the instrument reads all zeros on
   exactly the turns latency gets reported about.
3. Trim the four bloated tool descriptions (~680 tokens/turn, zero risk).
4. The latent constitution unit (item 4) — and close it with the same sweep that
   closed the others: `grep -rn "chars().count()"` over every producer feeding
   the ceiling. That one command found the whole class in a second; three rounds
   of prose asserting the class was closed did not.
5. Only then consider embedding-based tool selection — and gate it on measuring
   MiniLM recall@8 over real turns first. `regent-embed` already ships fastembed;
   `ToolCatalog.activated` is append-only, so reveals are cache-stable. Keep it
   strictly additive over `LIGHT_PINNED` so a recall miss degrades to today's
   behaviour rather than worse.
