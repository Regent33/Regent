# Cadence study — does explicit Anthropic prompt-caching pay, per surface?

**Date:** 2026-07-10 · **Trigger:** P1 prerequisite in
`docs/proposal/token-efficiency-architecture-v1.md` §3.2/§6 — the cadence gate
that decides whether the P2 Anthropic `cache_control` adapter should send
breakpoints at all, and if so, at which TTL. **Method:** direct SQLite query
against a copy of the live sessions store, no guesswork.

## TL;DR

Explicit caching pays on the surfaces where Regent talks to itself in tight
loops (`deacon`, `daemon`) and on human-paced chat (`telegram`). It
structurally cannot pay on `review` — the automated title-sweep surface is
94.5% single-turn sessions that never get a second request to read the
cache. Recommendation for P2:

| Surface | Send breakpoints? | TTL | Why |
|---|---|---|---|
| `deacon` | **yes** | **5m** | 96.6% of gaps land inside 5m; median gap 8s; long call-loop sessions (mean chain 10.7 turns) — 1h buys only +2.6pp for 2× the write cost |
| `daemon` | **yes** | **5m** | 95.7% of gaps land inside 5m; write+read economics clearly favorable (1.35 vs 2.0 baseline per pair) |
| `telegram` | **yes** | **1h** | slower human cadence (median gap 58s, p90 4.0m); 100% of gaps land inside 1h vs. 92.3% inside 5m — the only surface where 1h clearly wins |
| `review` | **no — send no breakpoints** | n/a | 0 of 660 sessions ever produce a second turn; a cache write here is pure 1.25×/2× overhead with no possible read |
| `delegate` | **no (insufficient data, defaults to off)** | n/a | 1 session on record, single-turn — same shape as `review`; revisit once volume exists |

Full numbers and the model used follow.

## Method

1. Copied `state.db` (+ `-wal`/`-shm`) from `~/.regent/` to an isolated
   scratch directory and analyzed the copy — the live deacon holds the
   original open, so it was never opened directly.
2. Queried `sessions` (`id, source, started_at, ended_at`) and `turns`
   (`id, session_id, started_at, ended_at`), grouped turns by session, and
   sorted each session's turns by `started_at`.
3. **Turns-per-session** = count of turn rows per session (sessions with no
   turn rows count as 0).
4. **Inter-turn gap** = for consecutive turns *i* and *i+1* in the same
   session, `gap = turn[i+1].started_at − turn[i].ended_at`. Only pairs with
   valid (non-null) timestamps and non-negative gaps are counted.
5. **Expected reads per cache write** — the model specified for this study:
   a write happens on every turn that has a next turn in the same session
   ("writing turns" = total turns − sessions with ≥1 turn, since the last
   turn of a session never gets a chance to be read). That write is read by
   turn *N+1* **only if** the gap to it is ≤ the TTL. So:

   ```
   expected reads per write = (gaps ≤ TTL) / (writing turns)
   ```

   This is a **conservative, single-hop model**: it credits at most one read
   per write (the immediate next turn only). It does not credit further
   reads deeper into a chain, even though Anthropic refreshes the cache TTL
   on every read — a session with several rapid-fire turns in a row can draw
   many reads off one original write. Where that matters, it's called out
   per surface below using **mean chain length** (average turns/session
   among sessions with ≥2 turns) as a proxy for how much better the real
   number likely is.

6. **Verdict economics.** Comparing a write-then-maybe-read pair (2 requests)
   against the uncached baseline (2 requests × 1×):
   - **5m TTL** (write = 1.25×): one read already wins — `1.25 + 0.1 = 1.35`
     vs. `2.0` baseline. Verdict bands: expected reads ≥0.9 → **pays**;
     0.5–0.9 → **marginal**; <0.5 or structurally 0 → **does not pay**.
   - **1h TTL** (write = 2×): a *single* read is actually a narrow loss —
     `2 + 0.1 = 2.1` vs. `2.0` baseline. 1h only pays once a write is read
     **more than once** within the window (`2 + 0.2 = 2.2` vs. `3.0` for a
     3-turn chain, clearly ahead). This is why 1h TTL is recommended only
     where chains are both long *and* gaps sometimes exceed 5m but stay
     under 1h (telegram) — for tight-loop surfaces (deacon/daemon), 5m
     already captures nearly all the benefit at a lower write premium.

## Sanity notes

- **Total turns analyzed:** 1,523, across 1,047 sessions. 688 turns are
  "writing turns" (have a next turn in the same session) and anchor the
  expected-reads computation; 688 gaps were measured (1:1 with writing
  turns, as expected).
- **Date range:** 2026-06-18 to 2026-07-10 (~22 days of ground truth).
- **212 of 1,047 sessions (20.3%) have zero turn rows** — created but never
  produced a completed turn. Breakdown: `daemon` 156 (56.5% of all daemon
  sessions!), `deacon` 20, `review` 36. For daemon, 151 of those 156 also
  have `message_count = 0` — these look like empty/placeholder sessions
  (health checks, aborted starts) rather than real conversations. They
  correctly contribute nothing to the gap/expected-reads math (the model
  only counts sessions with turn rows), but they do mean "sessions" is not
  the same population as "sessions that ever talk" for daemon — read the
  daemon verdict as conditional on a session reaching its first turn at all.
- **No NULL `started_at`/`ended_at`** in the 1,523 turns — clean, no
  timestamp gaps to paper over.
- **Small samples flagged:** `telegram` (10 sessions, 26 gaps) and
  `delegate` (1 session, 1 turn) are too small to be high-confidence.
  Telegram's verdict is directionally sound (100% of its gaps land inside
  1h) but should be revisited once more volume accumulates. Delegate has no
  usable signal at all — defaulted to "no breakpoints" because its one
  session was single-turn, the same shape as review.
- Negative gaps (clock skew / out-of-order rows) are guarded against in the
  script; none were observed in this dataset.

## Turns-per-session distribution

| Surface | Sessions | 0-turn | 1-turn | ≥2-turn | min | median | p75 | p90 | max |
|---|---|---|---|---|---|---|---|---|---|
| **Overall** | 1,047 | 212 (20.3%) | 721 (68.9%) | 114 (10.9%) | 0 | 1 | 1 | 2 | 54 |
| `daemon` | 276 | 156 (56.5%) | 62 (22.5%) | 58 (21.0%) | 0 | 0 | 1 | 2 | 54 |
| `deacon` | 100 | 20 (20.0%) | 31 (31.0%) | 49 (49.0%) | 0 | 1 | 7 | 17.1 | 37 |
| `review` | 660 | 36 (5.5%) | 624 (94.5%) | 0 (0.0%) | 0 | 1 | 1 | 1 | 1 |
| `telegram` | 10 | 0 | 3 (30.0%) | 7 (70.0%) | 1 | 3 | 5.75 | 7 | 7 |
| `delegate` | 1 | 0 | 1 (100%) | 0 | 1 | 1 | 1 | 1 | 1 |

`review` never produces a second turn in this entire dataset — 660 sessions,
zero exceptions. `deacon` is bimodal: a fifth of sessions produce no turn,
another third are single-shot, but half chain to a median of 7 turns and up
to 37 (Butler-style call loops). Mean chain length among ≥2-turn sessions,
for reference: overall 7.0, daemon 4.2, deacon **10.7**, telegram 4.7.

## Inter-turn gap distribution

| Surface | Gaps measured | median | p75 | p90 | ≤5m | ≤1h |
|---|---|---|---|---|---|---|
| **Overall** | 688 | 10s | 27s | 1.3m | 96.2% | 99.3% |
| `daemon` | 185 | 19s | 45s | 2.2m | 95.7% | 99.5% |
| `deacon` | 477 | 8s | 20s | 45s | 96.6% | 99.2% |
| `telegram` | 26 | 58s | 2.1m | 4.0m | 92.3% | 100.0% |
| `review` | 0 | — | — | — | — | — |
| `delegate` | 0 | — | — | — | — | — |

`deacon`'s gaps are the tightest by far (p90 under a minute) — consistent
with automated call loops rather than a human waiting between turns.
`telegram` is the slowest-paced surface (median 58s, some gaps stretching to
4 minutes) but still comfortably clears 1h at 100%.

## Expected reads per cache write

Using the single-hop model from §Method (gaps ≤ TTL ÷ writing turns):

| Surface | Writing turns | Reads ≤5m | Reads ≤1h | Expected reads @5m | Expected reads @1h |
|---|---|---|---|---|---|
| **Overall** | 688 | 662 | 683 | 0.962 | 0.993 |
| `daemon` | 185 | 177 | 184 | 0.957 | 0.995 |
| `deacon` | 477 | 461 | 473 | 0.966 | 0.992 |
| `telegram` | 26 | 24 | 26 | 0.923 | **1.000** |
| `review` | 0 | 0 | 0 | n/a (structurally 0) | n/a (structurally 0) |
| `delegate` | 0 | 0 | 0 | n/a (1 session, no signal) | n/a |

## Verdict per surface

| Surface | Verdict @5m | Verdict @1h | Recommended TTL | Recommendation |
|---|---|---|---|---|
| `deacon` | pays (0.966, well above the 0.9 bar) | pays (0.992) but only +2.6pp over 5m | **5m** | write breakpoints, 5m TTL |
| `daemon` | pays (0.957) | pays on paper (0.995) but the write premium doubles for the population most likely never to chain (56.5% zero-turn) | **5m** | write breakpoints, 5m TTL |
| `telegram` | marginal-to-pays (0.923) | pays cleanly (1.000 — every observed gap is inside 1h) | **1h** | write breakpoints, 1h TTL (small sample — revisit as volume grows) |
| `review` | does not pay (0 writing turns, ever) | does not pay | n/a | **send no breakpoints** |
| `delegate` | insufficient data | insufficient data | n/a | **send no breakpoints** (defaults off; revisit once this surface has volume) |

The `deacon` and `daemon` picks favor 5m over 1h even though both clear 1h
at slightly higher hit rates, because the 1h write premium is 2× vs. 1.25×
and the gap distributions are already so tight (deacon p90 = 45s, daemon p90
= 2.2m) that 1h buys almost nothing extra. `telegram` is the mirror case:
its slower human cadence means a meaningful slice of gaps (7.7%) sit between
5 and 60 minutes, so the cheaper 5m TTL would let real cache-eligible turns
expire unread — 1h is worth the extra write cost there.

`review`'s verdict is definitive, not modeled: 660 sessions and precisely
zero of them ever reach a second turn. Sending Anthropic breakpoints on this
surface would add pure 1.25×/2× overhead to every request with no
mechanism, ever, to recoup it from a read. The P2 adapter should treat
`review` (and, until it has data, `delegate`) as a hard no-breakpoints
surface rather than relying on the runtime `expected reads < 1` gate from
§3.2 to catch it turn-by-turn — the gate would technically also catch this
correctly (0 < 1), but coding the surface exclusion explicitly avoids paying
for the detection attempt on a surface already known to never pay.

## What this means for P2

- Wire the cadence-gate default from measured per-surface expected-reads,
  not a single global constant — `review` and `deacon` are opposite ends of
  the same product, and one `cache_control` policy for both would either
  waste money on `review` or under-serve `deacon`.
- 5m TTL is the safe default for the two Regent-internal surfaces
  (`deacon`, `daemon`); 1h is worth reserving for surfaces with genuinely
  human-paced cadence (`telegram`, and by extension any future chat-style
  surface with a similar gap shape).
- Because this study's expected-reads model is a conservative lower bound
  (single-hop, no credit for TTL-refresh-on-read chains), the "pays"
  verdicts above should be treated as floors — the deacon call-loop sessions
  in particular (mean chain length 10.7, gaps almost all under a minute)
  likely realize meaningfully more than one read per write in practice. The
  §3.3 prefix-hash telemetry (provider-reported `cache_read_input_tokens`)
  is what will confirm the realized number once P2 ships — this study only
  had to clear the bar of "expected reads ≥ ~1," not measure the true
  multiplier.
