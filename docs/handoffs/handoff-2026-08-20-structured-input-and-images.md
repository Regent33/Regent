# Handoff — structured user input + inline images (2026-08-20)

**Status: investigation and plan complete. No implementation has started.**
Nothing in this handoff claims tested behaviour, because no behaviour was changed.

**Plan:** [`docs/plans/structured-input-and-inline-images.md`](../plans/structured-input-and-inline-images.md)

---

## What was requested

1. A general-purpose **interactive questionnaire / clarification primitive** — single
   select, multi-select, free text, confirm, ranked — usable by any agent or tool, in
   **Regent CLI** and **Regent App chat**, and translated to the best native UI on
   messaging platforms. Priority: **Telegram → WeChat → Discord → the rest.**
2. **Native image viewing inside Regent App chat** — pictures rendered in the
   conversation instead of bare URLs or paths.
3. This handoff plus a plan in `docs/plans`.

Also standing from the same session: **message reactions** (long-press emoji) on gateway
platforms, same platform order. That is a separate, smaller piece of work — see
"Related, not covered" below.

## Why these are needed

`ask_user` today can only ask an open question and take a sentence back. Every choice the
model wants to offer becomes prose, and every answer becomes free text it has to
re-interpret. That is the fragile plain-text path — it produces mis-parsed answers and
gives the user no affordance to just pick option 2. The image gap is simpler: the app
already renders markdown images, so a model that produces a *local* file path (the common
case for camera capture, generated documents and artifacts) shows the user a broken
image or a raw path.

## What was investigated

Read end-to-end, not skimmed: the `ask_user` tool and its approval round-trip through the
deacon and the gateway; both surfaces' answer paths; the Desktop transcript model and
markdown renderer; the attachment staging pipeline; the artifacts RPC; the Tauri CSP; the
Telegram/WeChat/WeCom/Discord adapters; the CLI's Ink component inventory; and the
repo's plan/handoff conventions.

## Existing architecture discovered

**The pause-ask-resume lifecycle already exists and works.** This was the single most
important finding — it means this feature is an *extension*, not a new subsystem.

- `regent-tools/src/infra/ask_user.rs` — `ask_user{question, context}` blocks on
  `ctx.approval.request(...)` and maps `ApprovalDecision` to `{"answer": …}`.
- `regent-deacon/.../session_manager/hooks.rs:28` — `RpcApprovalHandler` parks a
  `oneshot` in `SessionEntry.approval_pending`, emits `approval.request`, denies after
  120s.
- `regent-deacon/.../dispatcher/session_ops.rs:284` — `approval.respond
  {session_id, approved, feedback?}`.
- `regent-gateway/src/application/approval.rs:76` — `ChatApprovalHandler` does the same
  over chat text, with per-tool grace coalescing.
- `.../session_ctx/approval.rs:21` — auto-approve **deliberately exempts** `ask_user`.

**Six concrete defects/limits found in the current design** (all cited with line numbers
in plan §1.3): no structure beyond two strings; **the Desktop app never sends the
`feedback` field, so it physically cannot answer an `ask_user` question with text**;
`approval_pending` is a single slot so a second question silently orphans the first;
nothing survives a restart; `env_auto_approver()`'s `AllowAll` path lacks the `ask_user`
exemption that `ConfigGatedApprover` has, so a voice deacon answers every question "yes";
and Telegram has no button support at all.

**Images are further along than expected.** `Markdown.tsx:97` already routes `img` to
`ZoomableImage`, which already has a click-to-open lightbox with Esc-to-close. So
`![](https://…)` already works. What is missing is local paths (no `asset:` in the CSP
and no fs access in the webview), attachment thumbnails (names render as chips), and
loading/error states. `dispatcher/artifacts_ops.rs` already solves the local-bytes
problem for the Artifacts window — base64 data URI, 5 MB cap, canonicalized within-root
check — so the image work copies a pattern that already exists rather than inventing one.

**CLI has a reusable list already.** `features/setup/presentation/SelectList.tsx` is a
windowed arrow-key list (`SelectRow {label, hint}`, render-only, parent owns `useInput`).
It is the right base for the CLI question UI; only 4 files in the whole CLI call
`useInput`, so keyboard ownership is easy to reason about.

**The reference implementation was read** (`D:\1-1@k\1-1Claudecode\ClaudeCodesrc\src` —
`tools/AskUserQuestionTool/`, `components/permissions/AskUserQuestionPermissionRequest/`,
`components/CustomSelect/`). Three things were adopted: the per-question `header` chip,
an always-present free-text row with the model told never to author its own "Other"
option, and the state/navigation/input/render split. Three were rejected with reasons —
answers keyed by question text as a flat string map, `multiSelect: boolean` in place of a
kind enum (it cannot express rank or a typed confirm), and the file sizes (`select.tsx`
is 669 lines; this repo caps at ~250). Plan §11 records all six.

## Architectural decisions made

1. **Extend the approval channel; do not build a parallel one.** The lifecycle is already
   correct in three surfaces. Duplicating it is the expensive mistake here.
2. **New `question.request` / `question.respond` notifications rather than JSON stuffed
   into `reason`.** A shipped client renders `reason` verbatim — a JSON blob would appear
   as raw text to real users, and the answer would return untyped.
3. **Capability negotiation on `session.create`.** No `capabilities: ["questions"]` →
   the deacon renders numbered text down the existing path. Today's CLI and app keep
   working against a new deacon, unchanged.
4. **Types live in `regent-kernel`** — the only crate that regent-tools, regent-deacon and
   regent-gateway all already depend on.
5. **`PlatformAdapter::ask_question` defaults to `Unsupported`,** exactly like the
   existing `send_file`. Ten adapters keep working with zero per-adapter code.
6. **Selection order doubles as ranking** — one `Answer::Selected` variant covers
   single-select, multi-select and rank instead of three near-identical shapes.
7. **Pending-question persistence is explicitly out of scope** for the first release. It
   changes turn-resume semantics for approvals too and deserves its own ADR.

## Files changed

**None.** Two documents were added:

- `docs/plans/structured-input-and-inline-images.md`
- `docs/handoffs/handoff-2026-08-20-structured-input-and-images.md` (this file)

## Files still needing changes

13 to create, ~16 to modify — the full list with per-file responsibility is the appendix
of the plan, and each is assigned to a phase in §10.

## Tests executed / results

**None for this work — no code was changed, so there was nothing to test.**

For context, the repo was left green by the immediately preceding work in the same
session (release packaging): `cargo fmt --all -- --check`, `cargo clippy --workspace
--exclude regent-voice-server --all-targets` with `-D warnings`, `cargo test --workspace`
(71 suites, 0 failures), CLI `tsc` + `biome` + 240 tests, the python parity and
release-tool suites, `verify-install.ps1` (44) and `verify-install.sh` (19), and the
Installer crate's own 14 tests. That is the baseline the first implementation phase
starts from — it is not evidence about this feature.

## Known issues / risks

- **Telegram `callback_data` is capped at 64 bytes.** Three composed ids will exceed it
  if ids are generated naively. The plan mandates short generated ids plus deterministic
  truncation with a hash suffix — get this wrong and buttons fail at runtime only.
- **Discord needs two surfaces to cooperate.** Component interactions arrive on the
  Ed25519 interactions webhook (`regent-deacon/src/infra/discord_interactions.rs`), not
  the WebSocket adapter that posted the message. Without the webhook configured, Discord
  must fall back to numbered text. This is a deployment prerequisite, not a code bug.
- **WeChat Official Account genuinely cannot do per-message buttons.** Numbered text is
  the ceiling; WeCom's `template_card` is the one that gets real buttons. Do not promise
  parity.
- **Gateway parity drift is a live hazard in this repo.** `regent-gateway` is a second
  composition root and has silently diverged from the deacon before (fixed in `157cbde`).
  Anything added on one side must be added on the other in the same change.
- **The single `approval_pending` slot** must become a map (or the sequential "1 of 3"
  flow must be strictly serialised) before multi-question runs are enabled, or the first
  question orphans and times out.
- **`REGENT_AUTO_APPROVE` currently auto-answers questions** on voice sessions. Fix it in
  Phase 2 or voice will silently agree to everything the model asks.
- **Parallel sessions run against this repo.** Re-read files before editing and check
  `git log` before committing.

## Remaining work

All ten phases in plan §10. Nothing is started.

## Recommended next action

**Phase 1 only, then stop for review.** Add the `regent-kernel` questionnaire types,
`validate()`, serde round-trip tests, the two TypeScript mirrors, and the parity script
in CI. It touches no behaviour, cannot break a surface, and locks the contract that all
nine remaining phases depend on — which is exactly the thing that is expensive to change
later.

Do **not** start with the UI. The card is the fun part and the least reversible if the
schema underneath it is wrong.

## Related, not covered here

**Message reactions** (long-press emoji) on gateway platforms, requested in the same
session with the same platform order. Independent of this plan, and smaller. The
prerequisite is the same missing field in both cases: `MessageEvent`
(`regent-gateway/src/domain/entities.rs:3`) carries `{platform, chat_id, user_id, text}`
with **no `message_id`**, so Regent cannot reference a specific message to react to it,
reply to it, or edit it. `OutboundMessage` has the same gap. Adding `message_id` is the
shared foundation for reactions, replies, and Telegram multi-select (which edits its own
message). Worth doing once, deliberately, before either feature.

Note for whoever picks that up: Telegram delivers `callback_query` by default but does
**not** deliver `message_reaction` unless it is named explicitly in `allowed_updates` on
the `getUpdates` call (`telegram.rs:97` currently sends only `{offset, timeout}`).
