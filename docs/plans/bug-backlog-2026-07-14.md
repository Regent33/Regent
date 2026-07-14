# Bug backlog — reported 2026-07-14 (fix after launch prep)

Owner-reported. Fix session instructions: use the senior-engineer workflow
(understand → plan → gate → execute → verify → report) with CLI/Rust
discipline and lazy-minimal diffs; **keep every file under ~200 lines, split
if it exceeds**. After the fixes, run **three review loops** — each loop reads
the diff as if fresh, hunts bugs, and verifies logic/code correctness before
the next.

## Bugs

1. **`regent agent edit <name>` (CLI/TUI) is not usable.** Can't view the full
   agent, can't save changes, and there's no TUI surface showing the editable
   fields: Name, Description, System Prompt, Model, Tools, Skills.
2. **Desktop app has no dedicated Settings page for agents** exposing the same
   full agent editor (Name, Description, System Prompt, Model, Tools, Skills).
3. **Desktop chat replies get cut off mid-answer** — the owner has to say
   "proceed" to continue. Investigate the session logs around the owner's
   "proceed please" messages; check streaming/turn-limit/timeout errors.
4. **Self-learning loop accuracy:** a stated preference ("I always like
   feature-based clean architecture as my folder architecture") was not
   recalled correctly a few sessions later. Verify the memory capture →
   retrieval loop end-to-end (episode capture, tri-modal recall, curator
   pruning) and whether token-efficiency trimming is dropping preferences.
5. **doc-forge not working:** Regent can't actually generate PowerPoint, PDF,
   Word, Excel, etc. — the skill exists but output fails. Reproduce and fix.
6. **Memory Home setting can't be edited/saved** — neither in Desktop app
   settings (no real-time save) nor via the CLI. Related open question: can
   the user choose where `~/.regent` lives from CLI/app settings (today it's
   only the `REGENT_HOME` env var / `-p` profiles)? Make it a first-class
   setting if feasible (requires daemon restart semantics).

7. **First-run wizard gate is defeatable (found + test-pinned 2026-07-14):**
   any command that boots the deacon (e.g. `regent model list`) seeds a full
   `config.yaml`, so `router.ts`'s `existsSync(config.yaml)` gate skips the
   wizard forever — the user lands in chat with no key and no guidance.
   Fix: gate on a wizard-written marker (or `.env`/provider presence), not on
   config existence. Pinned by the `test.todo` in
   `src/regent-cli/src/features/setup/cli/setupCommand.int.test.ts`.
   Smaller onboarding polish, same surface: API key is typed visibly (warned,
   but no masking); no non-TTY notice when defaults are auto-accepted; model
   name is free text with no validation until first call; `--constitution`
   flag parsed but dead.

8. **Onboarding refinements (owner, 2026-07-15):** optional gateway-platform
   selection step (pick from the 17 adapters, write their env-var names to
   `.env` as placeholders); optional local voice (ASR/TTS) setup step that
   offers the model auto-download (~900 MB) only on explicit agreement.
   ~~King art on the wizard header~~ done 2026-07-15.

## Investigation results (2026-07-15, log forensics on ~/.regent/logs)

**#3 chat cutoffs — diagnosed.** Three contributors found in the logs:
(a) `turn budget exhausted — wrap-up summary returned api_calls=91` — main
chat hits `AgentConfig.max_iterations = 90` on long agentic turns, wraps up
by design (Gap L2), and waits for the user's "proceed";
(b) `api_calls=9` — background review sessions capped at 8 (by design);
(c) `compaction ineffective — circuit breaker open` followed by `session
split complete` right before one cutoff — the app may lose the thread on the
child session. **Fix path:** expose `Agent.last_turn_budget_exhausted` in the
deacon's turn-result payload, and have the Desktop chat render a one-click
"Continue" chip when set (auto-continue would defeat the runaway ceiling).
Verify the app follows `session split` children.

**#4 memory recall — partially audited.** Local (CLI/app) sessions commit
memory writes directly (`external: false` in `memory_tools/actions.rs`), so
approval-staging is NOT the drop point. Retrieval evals (golden_retrieval,
fusion_eval) pass. Remaining suspect: capture — whether a stated preference
in chat actually produces a memory node (episode capture extraction), and
whether the ledger reset bug (fixed 2026-07-15, atomic .usage.json) was
starving skill selection. Next step: instrument one session — state a
preference, then check `regent memory` for the node before blaming recall.

**#5 doc-forge — no failure evidence in the logs** (no failed pptx/pdf turns
recorded). The `documents` skill is bundled+pinned and reaches the catalog.
Needs a live repro with the exact failing request; capture which lane
(python/JS/msedge) the agent picked and what the terminal tool returned.

## Notes

- §3 needs log forensics before code changes — capture the error signature
  first.
- §4 is a behavior audit, not necessarily a bug — measure recall on stated
  preferences before changing code.
