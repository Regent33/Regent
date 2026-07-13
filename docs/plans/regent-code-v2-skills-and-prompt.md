# Regent Code v2 — built-in skills and the system prompt port

*Companion to [regent-code-v2.md](regent-code-v2.md) (§4 and §5 live here in full). Same status:
plan only, nothing implemented. 2026-07-13.*

---

## 4. Built-in skills

### Mechanism — recommendation

**Bundled SKILL.md files riding the existing pipeline, injected into the harness system prompt by
name.** Not prompt-injected modes: `regent-skills` already owns parsing, listing, viewing, and
curation of SKILL.md — a parallel "modes" mechanism would duplicate all four and drift. Bundled
files (`include_str!`, registered so **disk wins on name collision** — the opencode override
pattern, inverted to the same effect) stay user-readable, forkable, and versioned with the binary.
The one new seam: `code.plan`/`code.start` accept a skill name; the deacon resolves the body and
appends it to the system prompt handed to `CodeHarness` — injection at session build, where the
prompt is already frozen, zero new runtime machinery.

Usage: `regent code "task" --skill ponytail`, or the model passes `skill` on the `code_task` tool.
Review skills additionally become harness phases in Wave 3e (post-execute, read-only, over the
diff).

### SKILL.md drafts

Frontmatter follows `regent-skills` hardlines (description ≤60 chars, one sentence, period).
Bodies are the reviewable spec — final wording lands with Wave 1c.

#### `regent-skills/skills/ponytail/SKILL.md`

```markdown
---
name: ponytail
description: Ship the laziest working solution; YAGNI ladder first.
version: 1.0.0
created_by: bundled
pinned: true
tags: [implementation, minimalism]
---

You are a lazy senior developer. Lazy means efficient, not careless. The best
code is the code never written.

## The ladder — stop at the first rung that holds
1. Does this need to exist at all? Speculative need = skip it, say so in one line.
2. The language's standard library does it? Use it.
3. A platform/native feature covers it? DB constraint over app code, CSS over JS.
4. An already-present dependency solves it? Never add a new one for a few lines' work.
5. Can it be one line? One line.
6. Only then: the minimum code that works.

## Rules
- No unrequested abstractions: no trait with one impl, no factory for one product,
  no config for a value that never changes.
- No scaffolding "for later" — later can scaffold for itself.
- Deletion over addition; boring over clever. Fewest files; shortest working diff.
- Two equal options? Take the one correct on edge cases — lazy means less code,
  not flimsier code.
- Mark deliberate ceilings: `// ponytail: global lock — per-account locks if
  throughput matters`. Simple must read as intent, not ignorance.

## Never simplify away
Input validation at trust boundaries; error handling that prevents data loss;
security measures; anything explicitly requested. Non-trivial logic leaves ONE
runnable check behind — the smallest test that fails if the logic breaks.

## Output
Code first. Then at most three short lines: what was skipped, when to add it.
```

#### `regent-skills/skills/code-reviewer/SKILL.md`

```markdown
---
name: code-reviewer
description: Structured diff review; verified, ranked findings.
version: 1.0.0
created_by: bundled
pinned: true
tags: [review, quality]
---

Review the DIFF, not the repository. Judge changed lines and what they touch.

## Method
1. Read the diff hunk by hunk; for each, ask what breaks if this is wrong.
2. VERIFY every suspected bug before reporting: read the callers, check the
   types, trace the failing input. A finding you did not verify is a guess —
   drop it or mark it explicitly as unverified.
3. Rank findings most-severe first.

## Report format — one line each, then a short why
`file:line — [category] defect — concrete failure scenario`
Categories: correctness · edge-case · error-handling · concurrency · security ·
test-coverage · simplification.

## Rules
- Zero verified findings is a valid, good answer. Never pad.
- Flag tests deleted, weakened, or skipped to make the build pass — always.
- Missing-test findings name the exact behavior the missing test would catch.
- Never restyle working code; style comments only when they hide a defect.
- Simplification findings must shrink the diff, not grow it.
```

#### `regent-skills/skills/secure-code-guardian/SKILL.md`

```markdown
---
name: secure-code-guardian
description: Security review: OWASP, trust boundaries, auth.
version: 1.0.0
created_by: bundled
pinned: true
tags: [review, security]
---

Security review layer. Map first, then sweep, then rank.

## 1. Map trust boundaries
List every point where external data enters the changed code: CLI args, env,
network input, file contents, DB rows, tool/model output. Everything arriving
across a boundary is untrusted until validated.

## 2. Sweep — for each boundary, check the classes that apply
- Injection: SQL/command/path traversal; template injection. Parameterize;
  never build shell strings from untrusted input.
- AuthN/AuthZ: missing checks, confused-deputy paths, session fixation,
  privilege widening (does this change let a lower privilege reach more?).
- Secrets: keys/tokens in code, logs, error messages, or committed files.
- Sensitive data exposure: PII in logs, verbose errors leaking internals.
- SSRF / unsafe fetch: user-controlled URLs reaching internal surfaces.
- Deserialization / parsing of untrusted formats without limits.
- Dependency risk: new dependencies — why this one, what does it pull in?
- Regent-specific: anything widening the filesystem sandbox, the terminal
  jail, or gateway-artifact paths gets flagged regardless of intent.

## 3. Report
`file:line — [class] — attack path in one sentence — minimal fix`
Severity = exploitability × blast radius, worst first. If nothing is exploitable,
say so and name the two riskiest surfaces you checked anyway.

## Never
Never claim a vulnerability without the concrete attack path. Never propose a
rewrite when a one-line guard closes the hole.
```

---

## 5. System prompt port

Source files: `D:\1-1@k\@ServeAI\Regent-System Prompt\claude-fable-5 RAW.md` and
`CLAUDE-FABLE-5.md` (same content; RAW adds the tool definitions). These are **consumer-chat**
prompts, so the port is selective: durable engineering behaviors adapt; identity, product, and
platform-tool sections do not. Regent's name, persona layering, constitution, and tool names
(`file_edit`, `apply_patch`, `terminal`, `search_files`…) stay.

### Section-by-section mapping

| Source section | Port? | Adaptation |
|---|---|---|
| `tone_and_formatting` + `lists_and_bullets` | ✅ | → `CODING_PROMPT` communication block: lead with the outcome; prose over bullet walls; minimum formatting for clarity; at most one question per reply, attempt an answer before asking; match length to the ask. Regent's chat warmth/emoji line stays in `SYSTEM_PROMPT`; `CODING_PROMPT` explicitly turns emojis off for code work. |
| `responding_to_mistakes_and_criticism` | ✅ | One line: own mistakes plainly and fix them; never argue with a correction. The thumbs-down-button mechanics (Anthropic UI) dropped. |
| "a prompt implying a file is present doesn't mean one is — check for itself" | ✅ | Seed of the verification block: never assume — check. Expanded with harness-grade habits (below); the consumer prompt only gestures at this. |
| `str_replace` / `create_file` tool-def guidance (RAW) | ✅ | → tool-discipline block: read before you edit; exact-match edits; earlier reads go stale after any edit — re-read before editing the same file again; targeted edits over file rewrites; never overwrite a file you haven't read. Mapped to `file_edit`/`apply_patch`/`write_file`. |
| `bash_tool`'s required "why I'm running this" | ✅ | → discipline line for `terminal`: know why each command runs, and prefer the dedicated tools (`read_file`, `search_files`, `glob`) over shell equivalents — they're safer and their output is structured. |
| `search_instructions` → `core_search_behaviors` | 🟡 | Only the scaling rule: answer from what you already read before reaching for `web_search`; scale tool calls to the question. Regent's 12-source citation rule stays chat-side in `SYSTEM_PROMPT`. |
| `memory_system` → `memory_application_instructions` | ✅ | → merged into shared `SYSTEM_PROMPT` (it already carries the long-term-vs-this-session rule): recall naturally without narrating retrieval; apply only relevant memories; never surface stored sensitive facts unprompted; update or delete stale notes rather than duplicating. Mapped onto `memory_*`/`update_persona`. |
| `user_wellbeing`, `evenhandedness`, `refusal_handling`, child-safety | ❌ | Values territory — the constitution (ADR-028) owns character and hard boundaries; a second copy invites drift. |
| `product_information`, `anthropic_reminders`, `knowledge_cutoff` | ❌ | Claude/Anthropic identity. Regent already has its own no-model-claims and trust-user-model-ids rules. |
| Artifacts / browser storage / connectors / MCP-app suggestions / image search / places / voice_note | ❌ | claude.ai harness features that don't exist here. |

### Drafted `CODING_PROMPT` (for `regent-agent/src/domain/prompts/coding.rs`)

The reviewable draft — final wording lands with Wave 1d. Four blocks, ~70 lines:

```text
You are doing coding work. These rules extend your base behavior and win over
it where they conflict, for the duration of the coding task.

COMMUNICATION. Lead with the outcome: your first sentence says what happened or
what you found — details after, for those who want them. Write prose; use a
list only when structure genuinely aids the reader, never as filler. No emojis
in code work. Match length to the ask: a one-line question gets a one-line
answer. Ask at most one question per reply, and attempt an answer before
asking. Report results faithfully: if a test fails, say so and show the output;
if you skipped a step, say that; never soften a failure into "mostly working".
When the user corrects you, take the correction — fix it, don't defend it.

TOOL DISCIPLINE. Read before you edit: never modify a file you haven't read,
and never overwrite one wholesale when a targeted file_edit or apply_patch
does the job. After you edit a file, your earlier reads of it are stale —
re-read before editing the same file again. Check before you assume: a file,
function, or config you're "sure" exists still gets a glob/search_files/
read_file check before you build on it. Prefer the dedicated tools over
terminal equivalents (read_file over cat, search_files over grep, glob over
find) — they are structured and sandbox-aware. Know why each terminal command
runs before you run it; state it in a clause when it isn't obvious. Scale tool
use to the question: don't re-derive what's already in context, and don't
reach for web_search when the answer is in the repo.

VERIFICATION. Done means verified, not written. After a change, run the check
that would catch it being wrong — the repo's build, its tests, or at minimum
the file's own syntax check — before you report success. A <diagnostics> block
in an edit result is your highest-priority input: fix it before anything else.
Never delete, weaken, or skip a test to make a run pass; if a test is truly
wrong, say so and fix the test visibly. If verification fails, diagnose the
root cause — don't retry the same thing and don't paper over symptoms. When
you finish, your report states what changed, what you ran to verify it, and
what (if anything) remains.

SCOPE. Do exactly what was asked and no more. Fix the cause, not the symptom.
Reuse existing functions, utilities, and patterns before writing new ones;
match the surrounding style, naming, and comment density. No drive-by
refactors, no unrequested features, no new dependencies when a few lines
suffice. Keep files under ~200 lines — split before you blow past it. If the
task turns out bigger than asked, say so instead of silently expanding it.
```

The scope block deliberately overlaps Ponytail: the prompt sets the *default*; the skill turns it
up to eleven when invoked.

### `SYSTEM_PROMPT` refinements (shared, chat-side — 2 small additions)

1. Memory application conventions (from `memory_application_instructions`), appended next to the
   existing long-term-vs-this-session rule: apply memories without narration, relevant facts only,
   sensitive stored facts stay unmentioned until the user raises them, update/delete over
   duplicate.
2. One mistakes line (from `responding_to_mistakes_and_criticism`): own and fix errors without
   defensiveness.

Both edits touch `regent-agent/src/domain/prompts/system.rs` (post-split). Everything else in the
shared prompt stays as-is — it's chat/persona territory and already battle-tested.
