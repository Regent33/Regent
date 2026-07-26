# Regent remediation plan — revised against the 2026-07-27 audit

Status: **P0 is a live regression. P3 shipped (`d1270d2`). Everything else PROPOSED.**
Supersedes the 2026-07-27 self-learning-only plan.
Inputs: `docs/research/hermes-vs-regent-comparison.md` (external audit, Hermes 41 / Regent 10)
plus this session's log investigation.

---

## P0 — Regent has had no shell. This is the coding-quality bug. *(my regression)*

Commit **`64aad1f` "jail every session to its cwd by default"** (Jul 26 11:34)
made `should_sandbox` default-on for local sessions. It did not account for
`terminal.rs:79`:

```rust
if ctx.is_sandboxed() && self.backend.describe() == "local" {
    return Ok(tool_error_json("terminal is unavailable in this jailed session: …"));
}
```

That guard was written for *untrusted ingress*. Making every session sandboxed
turned it into a blanket ban: **no ordinary chat session can run a shell command.**
No `npm install`, no build, no test, no verify.

### It is not inference — the agent documented it

Regent authored two skills to cope with its own crippled state:

- `jailed-terminal-fallback` — a five-step escalation ladder around the jail error
- `npm-background-build-verification`

`jailed-terminal-fallback` records **five** recurrences of one failed job
(`molecular-biology-site`, task_ids 99, 107, 165, 173, 172), each ending
"I'll report back … as soon as it completes" with nothing delivered, and notes
that `background_task` **inherits the same jail** so the escape hatch was never
real.

### This single cause explains four separate reports

| Report | Mechanism |
|---|---|
| "doesn't complete coding tasks, output is low quality" | cannot execute anything; only static file inspection was possible |
| "says the task is still running after it finished" | jailed background jobs never produced output. The dedupe + 45-min staleness fix (`a159d0a`) treated the symptom |
| `background_task` doom loops (4 in one day) | duplicate launches trying to escape the jail |
| memory/skills full of "terminal doesn't work" | the learning loop faithfully recorded a **false permanent constraint** — precisely the anti-lesson class P3 now blocks, written *before* P3 landed |

### Fix

Distinguish **why** a context is jailed. The file-scope containment the owner
asked for ("won't leak and edit files that are not in scope") is worth keeping;
banning the shell in the user's own deliberately-opened repo is not what was asked
for and makes the coding product non-functional.

- Jail from **external ingress** or explicit `REGENT_SANDBOX` → keep refusing the
  local shell. Untrusted input must not get a shell. Unchanged.
- Jail from the **default local hygiene rule** (`64aad1f`) → allow the local
  shell. Path resolution stays jailed for file tools; the shell is restored.

Smallest shape: carry the origin on `ToolContext` (one bool, set where
`should_sandbox` already decides) rather than re-deriving policy in the terminal.

**This is a security-posture line, so it is the owner's call to confirm** — but
the status quo is not the safe option, it is a broken product that also poisons
its own memory.

### P0b — retract the poisoned learning

`jailed-terminal-fallback` and `npm-background-build-verification` encode a
constraint that will be false the moment P0 lands, and skills outlive the bug that
created them. Archive both (`archive`, never delete — recoverable), and re-check
memory/persona for the same claim. **Owner consent required: this is their data.**

---

## 1. Corrections to the audit

The audit is careful and mostly right. Three of its seven follow-ups are unsound
and should not be prioritized as written.

| Audit item | Finding |
|---|---|
| **#4 "no dependency-scanning workflow; cheapest close of the largest security-process gap"** | **Wrong.** `.github/workflows/ci.yml:39-52` has a dedicated `supply-chain` job running `cargo-audit` (with one documented RUSTSEC ignore) and `cargo deny check` on every CI run. Nothing to build. |
| **#5 "terminal-tool → artifacts-jail bypass: known, flagged, unfixed"** | **Stale.** Fixed at `terminal.rs:73-86`, which cites the same 2026-07-13 flag date. The live defect is the opposite one (P0): the fix is now over-applied. |
| **#2 "split the memory block by stability (pinned + user profile in Tier-1)"** | **Rests on my error.** The audit inherited this from my own plan. Measured: pinning is `ttl_expires_at IS NULL` and *every* entry node already has no TTL (`memory` 6/6, `user` 4/4), so "pinned only" selects everything. See P1 below for the corrected split. |

Everything else in the audit holds, including its two most uncomfortable findings:
Regent's per-turn memory path **is** byte-identical to Hermes's, and **zero** of
the four performance criteria has been benchmarked on either side.

---

## 2. Memory & self-learning (revised)

### P1 — Split the memory block by **usefulness**, not pin state *(core)*
`access_count` is live and informative (270 hits across 6 memory nodes, 202 across
4 user nodes) and `retrieve.rs` already scores with recency decay.

- Prompt block: top-N by `(access_count, recency)` under a small fixed char
  budget → per-turn cost stops growing with the corpus.
- Per-turn: `retrieve(query, k)` + `render_recall(..)` — **both already exist and
  already serve `memory_search`** — injected outside the cached prefix.

**Ships whole or not at all.** Narrowing the prompt block alone makes every entry
outside the top-N reachable only if the model chooses to call `memory_search` —
a recall regression dressed as an optimization.

**Open decision (do not default):** where the per-turn block goes. The dispatcher's
existing decoration seam (`wrap_prompt`, `editor_note`) is cheapest but lands in
stored history, so it needs a `promptDecorations` stripper like attachments got.
A seam inside `regent-agent`'s turn assembly keeps history clean but touches the
hottest path.

### P2 — Hard cap → curation trigger. **Must land after P1, never before.**
Past a *soft* limit the reviewer consolidates/merges/evicts by value; the tool
stops refusing writes. Loosening the cap while the prompt still injects everything
would grow every prompt without bound. Store is at **75% of 2200 with six
entries**, so this is close.

### P4 — Usefulness-ranked eviction *(cheaper than planned)*
`access_count` **and** `last_accessed_at` already exist on `nodes` and are already
incremented on read (`regent-store/src/infra/graph.rs:165`). No schema change.
Evict by `(hits, age)`. Neither system knows which lessons paid off — this is
where Regent passes Hermes rather than catching up.

### P5 — Semantic contradiction handling
`add_node` dedupes by content **hash**, so a paraphrase of a changed preference
appends alongside the stale one. Extend to near-duplicate (cosine above threshold)
→ replace.

### ~~P3~~ — **SHIPPED** (`d1270d2`)
Anti-lesson guardrails in `REVIEW_SYSTEM_PROMPT`. Note the timing: the skills in
P0b are exactly what P3 exists to prevent, and they were written before it landed.

---

## 3. Tracks the audit adds

### A — Benchmark harness *(was P6; promote)*
Every performance claim on both sides is inference. Hermes is installed locally,
so this is available today: memory-path tokens/turn, recall@k on a seeded corpus,
writes-before-saturation. **Do this early** — it is what converts the rest of this
plan from opinion into measurement.

### B — LSP client for code context *(large, highest capability ceiling)*
The audit's strongest technical finding: Hermes has a full LSP client (11 modules)
versus Regent's text search. It is the one investment that moves context
awareness, debugging, and multi-file coherence simultaneously. Not before P0/P1.

### C — Explicitly deprioritized
Supply-chain CI (exists), terminal jail bypass (fixed). Skills breadth (16 vs 180)
and platform breadth are surface-area gaps that follow from 1 maintainer vs ~108 —
not defects to fix in a sprint.

---

## 4. Order

1. **P0** — restore the shell. Blocks all coding work; nothing else matters while it holds.
2. **P0b** — retract the poisoned skills (needs owner consent).
3. **A** — benchmark harness, so P1/P2 are measured rather than argued.
4. **P1 + P2** together, in that order.
5. **P4**, then **P5**.
6. **B** (LSP) as a separate, scoped project.

## 5. Verification

Repo gates: `cargo test --workspace --exclude regent-voice-server`,
`cargo clippy --all-targets`, `cargo fmt --check`; Desktop `bun run typecheck`,
`bun test`, `bun run build`.

P0 needs a **live** check, not just a green suite: open a real repo in a session
and run `npm --version` through the terminal tool. The regression shipped with a
full green suite — no test asserted that an ordinary session can run a command.
Add one that does.
