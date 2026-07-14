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

## Notes

- §3 needs log forensics before code changes — capture the error signature
  first.
- §4 is a behavior audit, not necessarily a bug — measure recall on stated
  preferences before changing code.
