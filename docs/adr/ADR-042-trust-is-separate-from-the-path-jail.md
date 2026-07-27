# ADR-042 — Trust is separate from the path jail

Date: 2026-07-27 · Status: accepted · Supersedes nothing; corrects `64aad1f`.

## Context

`ToolContext` had one boolean, `is_sandboxed()`, and three call sites read it as
three different questions. It answers "are resolved paths jailed to the working
tree". But `terminal` read it as "is this session's input untrusted" (refusing a
local shell), and `memory` read it the same way (staging writes for approval
instead of committing).

That was tolerable only while the jail was rare. `64aad1f` made it default-on for
every session. For one day, no ordinary session could run a command — no
`npm install`, no build, no test — and every `memory add` silently queued instead
of saving. Regent wrote itself five skills to cope with the shell it had lost,
one recording five recurrences of a single job that each ended "I'll report back"
with nothing delivered.

## Decision

Two independent facts, stored and asked separately.

- **`is_sandboxed()`** — paths are jailed. Default-on for every session. Kept.
- **`is_untrusted()`** — the turn came from outside the owner. True only for
  external ingress (platform webhook / gateway) and an explicit `REGENT_SANDBOX`
  run. Opening your own repo is not a trigger: the user is still the user.

`terminal` and `memory` key off trust. The gateway — a second composition root
that built a bare `ToolContext::new` and so had a full local shell on inbound
chat messages — is now marked untrusted at its own construction site.

## Consequences

Ordinary sessions get their shell and their memory writes back, and keep the
path jail they did not have before `64aad1f`. Against the pre-regression
baseline the posture is strictly better; against the regression it is a
deliberate, owner-approved relaxation.

**What this does not do.** `npm install` and `cargo build` execute repo- and
dependency-supplied code — `postinstall`, `build.rs`, git hooks — on the host,
under the owner's credentials. Nothing here inspects or contains that; the hole
predates the regression. Routing those to the existing ephemeral container
backend was considered and deferred as separate work.

Therefore: while an arbitrary host shell is reachable, *"don't edit files outside
scope, ask first"* is a **behavioral policy, not an enforced invariant.** Do not
claim enforcement. Enforcement needs filesystem mediation or a narrow explicit
host grant.

`untrusted` defaults to false, so it fails **open**: any future context built
outside `SessionManager::tool_context` or the gateway is trusted by default. New
ingress paths must mark themselves. The test
`tool_context_marks_external_ingress_and_only_external_ingress` exists because
deleting the marker otherwise leaves the suite green.
