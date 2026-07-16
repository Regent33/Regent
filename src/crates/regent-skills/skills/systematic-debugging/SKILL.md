---
name: systematic-debugging
description: "4-phase root cause debugging: understand bugs before fixing."
version: 1.0.0
created_by: bundled
pinned: true
tags: [debugging, troubleshooting, root-cause]
---

Random fixes waste time and create new bugs. **Core principle:** ALWAYS find
root cause before attempting fixes. Symptom fixes are failure.

## The Iron Law
```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```
Haven't completed Phase 1? You cannot propose fixes.

## The Feedback Loop Rule
Before reading code to build a theory, create or identify a **tight** command
that goes red on the user's exact symptom and green once fixed. Fast,
deterministic, agent-runnable, specific to this bug — not merely "doesn't
crash." When a clean repro is hard, spend disproportionate effort building
the loop; guessing without a red-capable loop is the failure mode this skill
prevents.

Use ESPECIALLY under time pressure, when "one quick fix" seems obvious, or
after 2+ failed attempts. Simple bugs have root causes too — don't skip.

## Phase 1: Root Cause Investigation

**Read errors carefully.** Full stack traces, line numbers, error codes.
`read_file` the relevant source; `search_files` for the error string.

**Build the tight loop.** Options, roughly in this order: a failing test at
the seam that reaches the bug; a curl/CLI script against a running instance,
diffing actual vs expected output; replaying a captured trace (log line,
request payload, queue message); a throwaway harness booting the smallest
useful slice; a bisection loop (`git bisect run`) if the bug appeared between
two known-good states; a differential loop comparing old vs new. For flaky
bugs, the immediate goal is a higher reproduction rate, not perfection — run
it 100x, add stress, narrow timing windows. A 50% flake is debuggable; 1%
usually isn't.

```bash
# Tight loop example
cargo test module::test_name -- --nocapture
for i in $(seq 1 100); do cargo test flaky_test || break; done
```

**Check recent changes.** `git log --oneline -10`, `git diff`, `git log -p
--follow path/to/file`.

**Multi-component systems** (API → service → DB): before proposing fixes,
add diagnostic logging at each boundary — what enters, what exits, config
propagation. Run once, find where it actually breaks, then investigate that
component.

**Trace data flow** when the error is deep in the call stack: where does the
bad value originate? Use `search_files` to trace callers and assignments
upstream. Fix at the source, not the symptom.

**Phase 1 done when:** errors are understood, a red-capable loop exists and
has run, recent changes are reviewed, and you can state root-cause
hypotheses — not just symptoms.

## Phase 2: Pattern Analysis

Once the loop is red, **minimize the repro**: cut inputs/config/steps one at
a time, re-running after each cut, until removing anything makes it green.
The minimal repro often becomes the regression test.

Find working examples of the same pattern elsewhere in the codebase
(`search_files`). If following a reference implementation, read it
completely — don't skim. List every difference between working and broken,
however small. Check dependencies: config, environment, assumptions.

## Phase 3: Hypothesis and Testing

Generate 3–5 falsifiable hypotheses before testing any. Rank by likelihood
and cheapness to falsify. Each needs a testable prediction: "if X causes it,
changing Y should make Z happen." If the user's present, show the ranked
list — they may re-rank it instantly with domain knowledge.

Test the top-ranked hypothesis with the smallest probe. One variable at a
time. Tag temporary debug logs with a unique prefix (`[DEBUG-a4f2]`) so
cleanup is one search.

Worked? → Phase 4. Didn't? → new hypothesis, don't stack fixes. Genuinely
stuck? Say "I don't understand X" and ask, don't fake it.

## Phase 4: Implementation

Write the failing regression test first (see `test-driven-development`).
Fix the root cause — ONE change, no "while I'm here" refactors. Verify:
run the regression test, then the full suite.

**Rule of Three:** fix doesn't work? Stop. Count attempts. Under 3 → back to
Phase 1 with new information. **3+ failed → stop and question the
architecture**, don't attempt fix #4. Signs of an architectural problem:
each fix reveals new coupling somewhere else, fixes need "massive
refactoring," each fix creates new symptoms. Discuss with the user before
continuing.

## Red flags — stop, return to Phase 1
"Quick fix for now, investigate later" · "just try changing X" · "I'll
manually verify" · "it's probably X" · "I don't fully understand but this
might work" · proposing fixes before tracing data flow · "one more attempt"
after 2+ failures.

## Common rationalizations

| Excuse | Reality |
|---|---|
| "Simple issue, skip the process" | Simple bugs have root causes too |
| "Emergency, no time" | Systematic is faster than guess-and-check thrashing |
| "Multiple fixes at once saves time" | Can't isolate what worked; risks new bugs |
| "I see the problem, let me fix it" | Seeing symptoms ≠ understanding cause |
| "One more attempt" (after 2+) | 3+ failures = architectural problem |

## Quick reference

| Phase | Activities | Success criteria |
|---|---|---|
| 1. Root Cause | Errors, tight loop, recent changes, evidence, data flow | Understand WHAT and WHY |
| 2. Pattern | Minimize repro, working examples, differences | Know what's different |
| 3. Hypothesis | Rank, test minimally, one variable | Confirmed or new hypothesis |
| 4. Implementation | Regression test, single fix, verify | Bug resolved, no regressions |

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
