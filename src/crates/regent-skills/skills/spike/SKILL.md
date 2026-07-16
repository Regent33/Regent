---
name: spike
description: "Throwaway experiments to validate an idea before build."
version: 1.0.0
created_by: bundled
pinned: true
tags: [spike, prototype, feasibility, exploration]
---

Use when the user wants to **feel out an idea** before committing to a real
build — validating feasibility, comparing approaches, or surfacing unknowns
research alone won't answer. Spikes are disposable by design.

Load when the user says "let me try this", "I want to see if X works",
"spike this out", "before I commit to Y", "is this even possible?", or
"compare A vs B".

**Don't use when:** the answer is knowable from docs/code (just research);
the work is production path (use a planning workflow instead); the idea's
already validated (go straight to implementation).

## Core method
```
decompose → research → build → verdict
   ↑___________________________↓  (iterate on findings)
```

### 1. Decompose
Break the idea into 2–5 independent feasibility questions. Each is one
spike. Present as a table, Given/When/Then:

| # | Spike | Validates | Risk |
|---|-------|-----------|------|
| 001 | websocket-streaming | Given a WS conn, when tokens stream, then client gets chunks <100ms | High |
| 002a | pdf-parse-a | Given a multi-page PDF, when parsed with lib A, then text is extractable | Medium |
| 002b | pdf-parse-b | Same question, library B | Medium |

**Comparison spikes** (002a/002b) share a number, different letter suffix.
Order by risk — the spike most likely to kill the idea runs first. Skip
decomposition only if the user already named one specific thing to spike.

### 2. Align
Present the table. Ask: "build all in this order, or adjust?" Let the user
drop/reorder before any code gets written.

### 3. Research (per spike, before building)
1. Brief it: 2–3 sentences, what/why/key risk.
2. If there's real choice of approach, table it: Approach | Tool/Library |
   Pros | Cons | Status (maintained/abandoned/beta).
3. Pick one, state why. If 2+ are credible, build quick variants.
4. Skip research for pure logic with no external dependency.

Use `web_search` to find candidates, `web_fetch` to read the actual docs.
For a library without hosted docs, `terminal` clone it and `read_file` its
README/examples.

### 4. Build
One directory per spike, standalone:
```
spikes/001-websocket-streaming/{README.md,main.rs}
spikes/002a-pdf-parse-a/{README.md,parse.py}
spikes/002b-pdf-parse-b/{README.md,parse.py}
```

**Bias toward something interactive.** A log line saying "it works" is a
weak spike. In order of preference: a runnable CLI with observable output; a
minimal HTML page demonstrating the behavior; a small server with one
endpoint; a test with recognizable assertions.

**Depth over speed** — never declare "it works" after one happy-path run.
Test edge cases; follow surprising findings.

**Avoid** unless the spike specifically needs it: heavy package management,
bundlers, Docker, config systems. Hardcode everything — it's a spike.

```
terminal("mkdir -p spikes/001-websocket-streaming")
write_file("spikes/001-websocket-streaming/README.md", "# 001: ...")
write_file("spikes/001-websocket-streaming/main.rs", "...")
terminal("cd spikes/001-websocket-streaming && cargo run")
# observe, iterate
```

Comparison spikes that both need real engineering: build them back to back
rather than interleaving, so the head-to-head comparison is fair.

### 5. Verdict
Each spike's `README.md` closes with:
```markdown
## Verdict: VALIDATED | PARTIAL | INVALIDATED
### What worked / What didn't / Surprises / Recommendation for the real build
```
VALIDATED = core question answered yes, with evidence. PARTIAL = works
under stated constraints. INVALIDATED = doesn't work, for this reason —
still a successful spike.

## Comparison spikes
After both are built, head-to-head:
```markdown
| Dimension | Approach A | Approach B |
|---|---|---|
| Quality | 9/10 | 7/10 |
| Setup complexity | one line | needs extra system deps |
| Perf on realistic input | 3s | 18s |

**Winner:** A for our use case. B if we need [specific constraint] later.
```

## Frontier mode (picking the next spike)
If spikes exist and the user asks "what next?", look for: integration risks
(two validated spikes that touch the same resource but were tested
independently); unproven data handoffs between spikes; capabilities assumed
but never spiked; alternative approaches for PARTIAL/INVALIDATED results.
Propose 2–4 candidates as Given/When/Then, let the user pick.

## Output
- `spikes/NNN-descriptive-name/` per spike, `README.md` + code
- Keep it throwaway — a spike needing 2 days of cleanup to reach production
  was a bad spike

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
