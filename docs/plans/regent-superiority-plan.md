# Regent superiority plan — baseline 0

Status (2026-07-27, end of day): **P0 · W1 · W2 · W6 · W3 steps 1–2 SHIPPED.**
P3 shipped earlier (`d1270d2`). Commits `87ea1ba` `c874e08` `1f646bf` `aa2f0c0`
`e366b44`. Remaining: W3 steps 3–7, W4, W5, W7–W12 — all still PROPOSED.

**Three claims in this plan were wrong and are corrected in place** (§5 item 1,
§7 W6, and the note below). Every one was inferred from logs or from the audit
and stated as a claim about source. Check the source before acting on a bullet
here.
Inputs: an external audit at `docs/research/hermes-vs-regent-comparison.md` —
**local-only, `docs/research/` is gitignored**, so that path is not in a clone;
its findings are restated here where they matter — plus source and log
verification on this machine, and a two-round adversarial co-audit by GPT 5.6 sol
(read-only: no tools, no execution — text in, critique out). Where the co-auditor
changed a decision it is marked **[co-audit]**.

Supersedes the earlier scored version of this plan. That version published an
adjusted scorecard (`Regent 12 / Hermes 39`). It is withdrawn: correcting a *fact*
the audit got wrong does not flip a *criterion*, and I had motive to conflate the
two — the audit was criticising my own system.

---

## 0. Baseline 0 — nothing is scored until it is measured

All **64** criteria (11 memory + 10 self-learning + 8 performance + 8 automations
+ 8 security + 9 orchestration + 10 coding) start **unscored**. Not 0–0 as a
draw: unscored, because no paired measurement exists for almost any of them.

*(The audit's own arithmetic is consistent at 64. An earlier draft of mine said 58
— my error, caught by the co-auditor.)*

**Scoreability gate [co-audit].** A criterion may be scored only when all five
hold. Spec frozen *before* results are inspected.

1. **Validity** — the measure represents the criterion, or the proxy was declared
   in advance.
2. **Symmetry** — both systems tested under one protocol, one failure rule.
3. **Completeness** — every declared case measured. No scoring the convenient
   subset of a composite criterion; split it instead.
4. **Reproducibility** — another run reconstructs the result from retained
   artifacts.
5. **Stability** — evidence maps to one score anchor. If the interval straddles a
   boundary, it stays unscored.

Inability to complete a test is a loss **only if** inability was a predeclared
outcome — never as an improvised penalty.

### Measured scale (re-counted here, not inherited)

Scale, not capability. Both trees, excluding `target/`, `node_modules/`, `dist/`.

| Metric | Regent | Hermes |
|---|---|---|
| Implementation | 85,608 Rust (702 files) + 38,301 TS/TSX | 1,640,884 Python (3,408) + 386,444 TS |
| Test | 12,254 LOC in `tests/` + 238 files w/ inline `#[cfg(test)]`; 1,102 test fns | 889,367 LOC (2,354 files); 44,589 test fns |
| CI workflows | 3 | 22 |
| Contributors | 1 (674 commits) | ~108 |
| Skills | 14 shipped | 181 `SKILL.md` |
| Providers | 19 `ProviderKind` | 33 plugins + 8 native |
| Exec backends | local, docker, ssh | + Singularity, Modal, Daytona |
| Direct deps | 46 Rust (723 resolved) + 52 npm | not counted |

**The audit's Regent figure of 279,033 Rust LOC is wrong — actual 85,608 (3.3×
over).** So Hermes is ~**16×** the implementation surface, not 6×. This correction
makes Regent look *worse*; it is recorded because the ratio is load-bearing for
every "we are smaller" argument, and an inflated denominator would let that
argument work too hard [co-audit].

---

## 1. What is actually measured today

The only paired or self-measured facts in hand:

- **Per-turn memory path is byte-identical to Hermes's** — 2,200 / 1,375 char caps,
  whole-corpus injection. Regent's RRF-fused retrieval serves the `memory_search`
  *tool*, not the block every turn pays for. Live store: **75% of ceiling at six
  entries**.
- **Provider failover amplified instead of mitigating — 8×.** See §5. FIXED
  2026-07-27: the multiplier was each adapter's 3 in-place retries × chain
  length, not the walk. A blind 429 now gets none.
- ~~**Dependency scanning is half-covered.**~~ CLOSED 2026-07-27 (§7 W6). The JS
  half was genuinely unwatched and genuinely not empty: a **high** React Router
  advisory in the shipped Desktop app, and **13 in regent-web (7 high), now 0**.
  "Container images" turned out to have nothing behind it — there are no
  Dockerfiles.
- ~~**Regent has had no local shell in any ordinary session.**~~ FIXED
  2026-07-27 (§3). Two further defects fell out of the same conflated boolean:
  every local `memory add` was silently queuing instead of saving, and the
  **gateway had an unjailed local shell on inbound chat messages** — older and
  larger than the regression itself.

Everything else in the audit — latency, throughput, cost, task success, recall — is
unmeasured on **both** sides. It stays unscored.

---

## 2. Audit corrections — facts, not scores

Five audit claims are wrong or stale. Each corrects the *record*; none flips a
criterion, because presence is not parity [co-audit].

| Audit claim | Correction | Does it flip the criterion? |
|---|---|---|
| "no dependency-scanning workflow" (its #4 priority) | `cargo-audit` + `cargo deny` gate CI | **No** — npm/Actions/images unscanned (§7 W6) |
| terminal→jail bypass "flagged, unfixed" | Fixed at `terminal.rs:73-86` | **No** — the live defect is the inverse (§3) |
| "split memory by stability (pinned + user profile)" | Rests on my error the audit inherited: pinning is `ttl_expires_at IS NULL` and every entry node has none, so the filter selects 100% | **No** — but the *concept* of a stable tier survives; only that implementation is impossible |
| `/learn` credited to Hermes | Regent ships `/learn` (`prompt_ops.rs:37-46`) | **No** — command-name parity is not capability parity |
| "local + Docker only" | local, docker, ssh | **No** — a backend count, not execution quality |

---

## 3. P0 — restore the shell, on an explicit capability model *(my regression)*

`64aad1f` "jail every session to its cwd by default" made `should_sandbox`
default-on for local sessions. A pre-existing guard (`terminal.rs:73-86`, written
for untrusted ingress) refuses a local shell in **any** sandboxed context. Applied
to every session it is a blanket ban: **no `npm install`, no build, no test, no
verify, anywhere.**

Not inference. Regent authored `jailed-terminal-fallback` and
`npm-background-build-verification` to cope. The first records **five** recurrences
of one job (task_ids 99/107/165/173/172), each ending "I'll report back" with
nothing delivered, and notes that `background_task` **inherits the same jail** — so
the escape hatch was never real. One cause behind four separate user reports: low
coding quality, "still running" after completion, background-task doom loops, and
skills encoding "terminal doesn't work" (the exact anti-lesson class P3 now blocks).

### The fix is not a boolean

My first design — record *why* a context was jailed — is rejected [co-audit]. Two
holes:

- **Origin is not enough.** An ordinary local session ingests hostile repo files,
  web content, tool output, and recalled memory. Taint must be **dynamic**, sticky
  across turns, must survive recall/summarisation/tool-output transforms, and must
  be **immutable from model-generated text**.
- **Operation class matters, and it is the uncovered attack.** `npm install`,
  `cargo build`, `make test` execute *repo- and dependency-supplied* code nobody
  inspected — `postinstall` hooks, `build.rs`, git hooks, compiler plugins. A
  malicious one reads `~/.ssh`, cloud credentials, env secrets, writes outside the
  repo, exfiltrates. Content taint computed at selection time cannot see this: the
  hostile code appears *during* execution.

So capability is a function of: explicit user grant · encountered-content
provenance · **operation class** · target resources · code the operation may
transitively execute. Operations that run repo- or dependency-supplied code
presumptively get the container backend (which already exists: ephemeral
`docker run`, no network, read-only rootfs, caps dropped, mem/pid capped, only
`/work` writable). Dependency *acquisition* may need a networked-fetch capability
followed by an offline build — otherwise every ordinary workflow pressures the user
into granting host access and the model is nullified [co-audit].

**Stated plainly, because the owner's requirement depends on it:** while an
arbitrary host shell is reachable, *"don't edit files outside scope, ask first"* is
a **behavioral policy, not an enforced invariant.** Textual approval cannot predict
what a nested script does. Enforcement needs filesystem mediation or a narrow
explicit host grant. This plan does not claim enforcement it does not have.

**Sequence** [co-audit] — test first, matrix before release, not before the fix:

1. Preserve the incident evidence (the two skills, the five task rows).
2. Minimal failing reproducer: an ordinary session cannot run a command.
3. Freeze the capability semantics and the confinement guarantee in writing.
4. Smallest fix; log which backend/capability was selected and why.
5. Targeted regressions: direct shell, background job, coding harness,
   local/docker/ssh, explicit `REGENT_SANDBOX`, external ingress, absolute paths,
   symlink traversal.
6. Archive — never delete — the two false-constraint skills, **excluded from
   retrieval**, linked to this incident. Owner's data, owner's call.

Security-posture line → **owner confirmation required.** The status quo is not the
safe option; it is a non-functional coding product that poisons its own memory.

---

## 4. W1 — Job lifecycle and verified completion *(the omission)*

**The largest gap in the previous plan** [co-audit]: it had result aggregation at
item 7 while holding direct evidence of jobs ending "I'll report back" with nothing
delivered. That is one systemic defect, not a cron-ledger feature.

Every foreground, background, cron, coding and delegated task gets: durable job id ·
state machine · attempt history · idempotency · artifacts · cancellation/timeout ·
completion evidence.

Completion is **four distinct facts**, never one boolean, and `inconclusive` is a
legal value [co-audit]:

1. process completed · 2. artifact produced · 3. result validated · 4. intended
outcome achieved.

Regent must not claim completion, nor promise a later report, without either a
verified result or a durable pending job that can actually deliver one.

Keep the core narrow — this is the natural place for a god abstraction to grow
[co-audit]. It unblocks: cron completion rate (D4a/b), skill efficacy (D2f/g),
benchmark data (§6), provider-failure diagnosis (§5), testable aggregation (D6h).

---

## 5. W2 — Provider admission control *(rescoped by measurement)*

I previously reported "276 HTTP 429s in one day." **That number was an artifact** —
`grep 429` was matching nanosecond timestamps and ONNX buffer sizes. Properly
scoped, eleven days of logs read:

| Day | rate-limited | failovers |
|---|---|---|
| 07-17 … 07-25 | 0–5 each | 1–16 each |
| **07-26** | **421** | **196** |

So: not chronic throttling. **One catastrophic day, and near-zero otherwise.**

The mechanism, from the logs: every rate-limit line *is* a chain hop
(`provider failed pre-stream; trying next in chain … error=rate limited (HTTP 429)`).
Three providers absorbed it — `poolside/laguna-s-2.1` 103, `z-ai/glm-5.2` 90,
`minimax-m3:cloud` 82 — and it clusters at **81 events in one minute, 78 the next,
69 later**, against ~50 provider selections that day. **Failover was amplifying the
outage ~8×, not mitigating it.** `Retry-After` appears **zero** times in any log:
it is never read.

The co-auditor's hypothesis — "the failovers may be amplification, not mitigation"
— is confirmed. This *shrinks* the work item from a learned per-provider limiter to
three precise things:

1. ~~**Honor `Retry-After`**. Currently ignored outright.~~ **WRONG — corrected
   2026-07-27 against source.** `run_with_retry` (`http.rs:28`) already prefers a
   server-stated `retry-after` over its jittered backoff, and has a test. It never
   appeared in the logs because those three providers do not *send* the header,
   not because the code ignores it. **A claim read off logs was published as a
   claim about source.** Real remaining gap: the parser takes numeric seconds
   only, so the RFC 7231 HTTP-date form is dropped, and provider-specific
   headers (`x-ratelimit-reset`, `anthropic-ratelimit-*`) are not read at all.
2. **Per-provider cooldown** so one chain walk cannot re-try a member that just
   429'd. *Existed* (flat 30s) but ignored the 429's own stated wait — a provider
   asking for 90s was re-hit at 30s. **Shipped 2026-07-27**: cools for the stated
   window, capped at 5 minutes.
3. **A retry budget per original request**, and a separate budget and breaker for
   failover itself, so it cannot amplify an outage. **Shipped 2026-07-27** as a
   hop cap: the real multiplier was `run_with_retry`'s 3 internal attempts ×
   chain length, so one turn could cost 3×N HTTP calls. Now first-provider + 2
   hops, applied *after* cooling members are filtered out so a healthy provider
   behind dead ones stays reachable. Still open: "fail over only to a destination
   with independent quota" — the three providers that absorbed 07-26 may share
   one, and nothing models that.

Before any tuning, record the denominators the aggregate hid: attempts vs original
requests, 429 ratio by provider/model/key, token volume, `Retry-After`, queue
depth, retry chain, failover chain, final outcome. Pacing handles the chronic
regime; a breaker handles the catastrophic one; neither substitutes [co-audit].

---

## 6. W3 — Memory: additive first, subtract last

Goal unchanged: stop injecting the whole corpus every turn. **"Ships whole or not
at all" is withdrawn** — it was a big-bang change defended as a safety argument
[co-audit]. The valid kernel of it survives: never narrow the static block before
automatic retrieval demonstrably covers what it removes.

Sequence — the *first* injection is canaried, not the rollout after it:

1. ~~**Instrument** the current path~~ **SHIPPED 2026-07-27** (`e366b44`).
   Block cost per session (entries/chars/percent) + per-turn shadow. Tokens and
   task outcomes are NOT yet captured — chars is the proxy in place.
2. ~~**Shadow retrieval**~~ **SHIPPED 2026-07-27**. Logs the candidate set, not
   just the selection. Two properties that turned out to be load-bearing and are
   now tested: it must not **touch** nodes (`retrieve` bumps `access_count`, so a
   touching shadow would manufacture the exposure-feedback loop this plan warns
   about) — hence `score_candidates` split out of `retrieve` — and it must not
   add turn latency, hence off the turn path. Opt-in via `REGENT_MEMORY_SHADOW`.
   **Gate for step 3: no data exists yet.** The flag has to run on real traffic
   before offline evaluation has anything to evaluate.
3. **Offline relevance / contradiction evaluation** on that log.
4. **Canary additive injection** — strict budget, kill switch, deduped against the
   existing block.
5. **Broaden additively.** Establish recall parity *before* removing anything.
6. **Canary supersession/dedup separately** — a different risk class from exact-
   duplicate removal.
7. **Reduce, then remove, whole-corpus injection** — only on measured
   non-regression for latent preferences, contradictions, task-relevant recall.

Both `retrieve` and `render_recall` already exist and already serve the tool.

**Ranking must not be `access_count` + recency.** That is an exposure feedback
loop: what is already injected accrues hits and stays injected. Outcome attribution
from W1 is necessary but *not sufficient* — it records correlation, and an injected
memory gets opportunities an un-injected one never had. Also required [co-audit]:
log the **candidate set** and not just the selection, randomized holdouts or
interleaving, and a no-memory control arm.

**Contradiction handling, not similarity replace.** `add_node` dedupes by content
hash, so a paraphrased preference change appends beside the stale one. But
"semantic near-duplicate replace" is the wrong primitive: *"I like X"* and *"I no
longer like X"* are near-identical vectors and opposite facts. Use temporal
supersession with explicit contradiction resolution — and preserve genuine
distinctions of subject, scope, source and context rather than forcing one current
fact. Tombstone by default; retain **real** deletion for privacy requests, secrets,
corrupt rows and retention limits, audited, with bounded tombstone compaction
[co-audit].

**W4 — soft cap → curation** replaces the hard 2,200 refusal, after W3 lands.
Versioned, reversible, and *proposes* merges and evictions before applying them.

---

## 7. Remaining tracks — each with its gate

| # | Track | Gate before building |
|---|---|---|
| W5 | **Skill curator.** `.usage.json` already records `use_count`/`view_count`/`state` (`library.rs:92-176`) and nothing consumes it | Offline **suggestion-only** generation may start now. Automatic promotion, ranking or patching waits for W1 attribution — use counts measure popularity, not efficacy [co-audit] |
| W6 | ~~**Close the dependency surface**~~ **SHIPPED 2026-07-27** (`aa2f0c0`). `bun audit` gates all four workspaces; dependabot covers cargo + Actions + npm. The hole was real: a **high** React Router CSRF advisory in the shipped Desktop app (accepted with owner/expiry/compensating control — it needs RSC mode, which a Tauri client-side router has no route to) and **13 advisories in regent-web, 7 high, now zero**. Both accepted advisories now carry an owner, an expiry and a compensating control. **Correction: "container images" had nothing behind it** — there are no Dockerfiles in the repo, only the user-supplied sandbox image. Still open: the 11 Actions are pinned by mutable tag, not SHA | Was: none — cheap. Confirmed cheap |
| W7 | **Cron executions ledger** | Falls out of W1; don't build a second ledger |
| W8 | **Threat-pattern scanning** on memory writes, tool results, context files | Ships as *one* layer of the §3 capability model. Pattern matching over text is not a prompt-injection boundary and must not be sold as one [co-audit] |
| W9 | **Orchestration**: concurrency cap, `interrupt_subagent`, real aggregation | Correctness parts (cancel, limits, durable results, failure propagation, status) come free with W1. Topology/parallelism breadth waits for a workload that demands it |
| W10 | **ANN index** (today brute-force O(N) cosine) | A measured **workload** crossover — query volume, latency SLO, recall loss, update rate, filter needs — not a corpus size. At six entries this is theater |
| W11 | **Episodic ±5-message session windowing** onto existing `session.search` | Cheap, self-contained, no gate |
| W12 | **LSP client** | Measured expected improvement on representative tasks vs cost. *Not* "it flips four criteria" — that is score optimisation, and one mechanism should not count as four independent wins [co-audit] |

## 8. Order

1. **P0** — restore the shell on the capability model. Nothing else matters while
   it holds.
2. **W1** — job lifecycle and verified completion.
3. **W2** — stop failover amplifying; honor `Retry-After`.
4. **W6** — close the npm/Actions/image dependency hole (cheap, parallelisable).
5. **W3** shadow → canary → additive, then **W4**.
6. **W5** suggestion-only curator; **W7** falls out of W1.
7. **W8**, **W11**, **W9** correctness half.
8. **W10**, **W12** — only if their gates open.

## 9. Not worth optimizing for score

Reworded from "unwinnable," which was inaccurate [co-audit].

- **Genuinely not reproducible by one maintainer:** institutional advisory traffic
  with real reporters; ~108 contributors.
- **Winnable, and moved *out* of this bucket:** regression coverage (raw test mass
  is vanity; coverage of the paths that broke is not — P0 shipped green because
  nothing covered it), and security *process* — disclosure policy, release
  signing, threat model, incident handling.
- **Engineerable but poor priorities:** skill, platform and provider counts.
  Grow on demand; never target the count.
- **Do not concede** anywhere in security: a shell regression adjacent to sandbox
  semantics makes boundary tests *more* important, not less.

Where a whole dimension is a vanity target, say so: **self-learning criteria that
reward autonomous skill mutation, promotion, memory rewriting and self-modification
are conceded unless they come with evaluation, versioning, review, rollback and
demonstrated outcome improvement.** Autonomous change is not better than reviewed
suggestion [co-audit].

## 10. Verification

Repo gates: `cargo test --workspace --exclude regent-voice-server`,
`cargo clippy --all-targets`, `cargo fmt --check`; Desktop `bun run typecheck`,
`bun test`, `bun run build`.

P0 additionally needs the §3 regression matrix plus a live check (open a real repo,
run a command). No claim from an unmeasured row ships as fact — §0 gate.

Cross-system benchmarking (recall@k, SWE-bench subset) is **not** a prerequisite for
any repair above, and is not neutral by default: same model, prompts, token and
time budget, tool permissions, retries, container, and grader — with the subset
**frozen in advance** so it cannot be cherry-picked [co-audit].
