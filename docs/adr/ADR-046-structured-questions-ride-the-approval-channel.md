# ADR-046: Structured questions ride the approval channel

**Date:** 2026-08-20 · **Status:** accepted

## Context

`ask_user` carried two strings. Every choice a model wanted to offer became prose
inside `context`, so every surface rendered a paragraph and every answer came
back as free text the model had to re-interpret. That is the fragile plain-text
path: mis-parsed answers, and no affordance for the user to just pick option 2.

The obvious shape — a second RPC channel with its own pending registry, its own
timeout, and its own interrupt semantics — would have duplicated a pause/ask/
resume lifecycle that already worked correctly in three surfaces (the deacon's
`RpcApprovalHandler`, the gateway's `ChatApprovalHandler`, and the CLI).

## Decision

**One typed payload travels beside the existing text; the machinery is unchanged.**

- The contract (`regent_kernel::contracts::questionnaire`) is **batch-shaped**: a
  `Questionnaire` carries all 1–5 of its questions and a surface answers them in
  one `QuestionnaireAnswer`. "1 of 3" is a client-side stepper over one request.
- `ApprovalHandler::request_structured` is a **default trait method** that renders
  numbered text down the existing `request` path and parses the reply.
- The deacon overrides it with `question.request` / `question.respond`, gated on
  `capabilities: ["questions"]` from `session.create`.
- Answers key by `Question.id` and return typed `Answer` variants — never a flat
  `question text → string` map. Selection **order is the ranking**, so one
  `Selected` variant covers single-select, multi-select and rank.
- The contract is hand-copied into two TypeScript files and held by
  `verify-questionnaire-schema.py` in the `parity` CI job.

## Consequences

- **All seventeen gateway platforms supported structured questions the moment the
  trait default landed**, with zero per-adapter code. This is the whole return on
  the default-method choice, and it is why platforms were freed to spend their
  effort on reactions instead.
- **Old clients keep working.** No capability → numbered text on a channel every
  shipped CLI and app already renders.
- **The single `approval_pending` slot stayed correct.** Batching means only one
  thing is ever in front of the human, so no map was needed.
- **Cost:** three copies of one contract, held together only by a CI script; and
  a text surface answering a multi-question card must put one answer per line.
- **Not decided here:** persisting a pending questionnaire across a deacon
  restart. It changes turn-resume semantics for approvals too and deserves its
  own ADR; today a pending question times out at 120s exactly as an approval does.
