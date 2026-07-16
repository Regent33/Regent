---
name: test-driven-development
description: "TDD: enforce RED-GREEN-REFACTOR, tests before code."
version: 1.0.0
created_by: bundled
pinned: true
tags: [testing, tdd, development, quality]
---

Write the test first. Watch it fail. Write minimal code to pass.
**Core principle:** if you didn't watch the test fail, you don't know if it
tests the right thing.

## When to use
Always: new features, bug fixes, refactoring, behavior changes. Ask the user
first for exceptions: throwaway prototypes, generated code, config files.
Thinking "skip TDD just this once"? That's rationalization — stop.

## The Iron Law
```
NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST
```
Wrote code before the test? Delete it — don't keep it "as reference," don't
adapt it while writing the test. Implement fresh from the test.

## RED — write a failing test
One minimal test, one behavior, clear name (name has "and" in it? split it).
Real code, not mocks, unless truly unavoidable — a test of mock behavior
tests nothing.

```rust
#[test]
fn retries_failed_operation_three_times() {
    let attempts = Cell::new(0);
    let result = retry(|| {
        attempts.set(attempts.get() + 1);
        if attempts.get() < 3 { Err("fail") } else { Ok("success") }
    });
    assert_eq!(result, Ok("success"));
    assert_eq!(attempts.get(), 3);
}
```

**Verify RED — mandatory, never skip.** Run it with `terminal`. Confirm it
fails for the *expected* reason (feature missing), not a typo. Passes
immediately? You're testing existing behavior — fix the test. Errors instead
of failing cleanly? Fix the error, re-run until it fails correctly.

## GREEN — minimal code
Simplest code that passes. Nothing extra — no logging, no "while I'm here."
Cheating is fine here: hardcode return values, duplicate code, skip edge
cases. REFACTOR fixes it later.

**Verify GREEN — mandatory.** Run the specific test, then the full suite.
Fails? Fix the code, not the test. Other tests broke? Fix the regression now.

## REFACTOR — clean up
Only after green: remove duplication, improve names, extract helpers. Keep
tests green throughout — don't add behavior. Tests fail mid-refactor? Undo
immediately, take smaller steps.

## Repeat — one vertical slice at a time
Don't write all tests first and then all implementation ("horizontal
slicing") — tests designed before the implementation teaches you the real
interface tend to be brittle. Use tracer bullets instead:

```
WRONG:  RED: test1,test2,test3    GREEN: impl1,impl2,impl3
RIGHT:  RED→GREEN: test1→impl1 → RED→GREEN: test2→impl2 → ...
```

## Why order matters
- **Tests written after code** pass immediately, which proves nothing — might
  test the wrong thing, might test implementation not behavior.
- **Manual testing** is ad-hoc: no record, can't re-run, easy to forget
  cases under pressure.
- **Sunk cost** ("already spent hours") isn't a reason to skip tests on code
  you can't trust — the time's gone either way.
- **Tests-after answer "what does this do?"** Tests-first answer "what
  should this do?" and force edge-case discovery before you implement.

## Common rationalizations

| Excuse | Reality |
|---|---|
| "Too simple to test" | Simple code breaks; a test takes 30 seconds |
| "I'll test after" | Tests passing immediately prove nothing |
| "Already manually tested" | Ad-hoc, not systematic; no record, can't re-run |
| "Keep as reference, write tests first" | You'll adapt it — that's testing after |
| "Test is hard to write" | Design is unclear — hard to test means hard to use |
| "Existing code has no tests" | You're touching it now — add tests for what you touch |

## Red flags — delete the code, start over
Code before test · test added after implementation · test passes on first
run · can't explain why a test failed · "just this once" · "already
manually tested it" · "keep as reference."

## Verification checklist
- [ ] Every new function has a test
- [ ] Watched each test fail before implementing, for the expected reason
- [ ] Wrote minimal code to pass each test
- [ ] All tests pass, output pristine (no warnings)
- [ ] Real code exercised, mocks only if unavoidable
- [ ] Edge cases and error paths covered

Can't check all boxes? You skipped TDD — start over.

## When stuck

| Problem | Solution |
|---|---|
| Don't know how to test it | Write the wished-for API/assertion first, or ask the user |
| Test too complicated | Design is too complicated — simplify the interface |
| Must mock everything | Code too coupled — use dependency injection |
| Test setup is huge | Extract helpers; still bad? simplify the design |

## With systematic-debugging
Bug found → write a failing test that reproduces it → debug (see
`systematic-debugging`) → the fix makes that test pass. Never fix a bug
without a test proving it.

*Adapted from Hermes Agent (MIT, © 2025 Nous Research).*
