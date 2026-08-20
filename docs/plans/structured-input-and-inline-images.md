# Structured user input + inline images — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development`
> (recommended) or `superpowers:executing-plans` to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Regent one structured way to ask a human a question — single-select,
multi-select, free text, confirm, rank — that works in the CLI, the Desktop app, and
platform chats, and make images render inside Desktop chat instead of as bare paths.

**Architecture:** Regent already pauses a turn to ask a human. The `ask_user` tool
blocks on the approval channel, and every surface already answers it. That lifecycle is
kept exactly as-is; what changes is the *payload* — a typed question object travels
beside the existing text so old surfaces keep working and new ones render a real UI.
Images reuse the artifact inlining path the Artifacts window already uses.

**Tech stack:** Rust (regent-tools / regent-deacon / regent-gateway), TypeScript + Ink
(regent-cli), React 19 + Vite + Tailwind tokens (regent-app/Desktop).

## Global constraints

- **No file over ~250 lines.** Repo convention (`docs/architecture-design/README.md`);
  every new component below is sized to fit or names its split.
- **Additive wire changes only.** A shipped CLI/app/gateway must keep working against a
  new deacon and vice versa. No field is removed or repurposed.
- **The gateway is a second composition root.** `regent-gateway` builds its own catalog
  and its own `ApprovalHandler`; anything added to the deacon side must be added there
  too or it silently drifts (this has already happened once — commit `157cbde`).
- **Constitution/values layer is always on** and is never bypassed by a new tool.
- **Platform order: Telegram → WeChat → Discord → the rest.**
- **The webview has no filesystem access** and the CSP has no `asset:` scheme. Local
  bytes reach the UI only through the deacon over JSON-RPC.
- `cargo fmt --all -- --check` runs FIRST in CI — a formatting slip fails the job before
  any test runs.

---

## 1. Current architecture

### 1.1 The ask-a-human lifecycle already exists

Regent can already stop a turn, ask, and resume. The path, end to end:

| Step | File | What it does |
|---|---|---|
| Tool | `src/crates/regent-tools/src/infra/ask_user.rs` | `ask_user{question, context}` — calls `ctx.approval.request("ask_user", question, context)` and maps the decision to `{"answer": …}` |
| Handler (deacon) | `src/crates/regent-deacon/src/application/session_manager/hooks.rs:28` | `RpcApprovalHandler` — creates a `oneshot`, parks it in `SessionEntry.approval_pending`, emits `approval.request`, waits up to `APPROVAL_TIMEOUT` (120s), denies on timeout |
| Wire out | same file, line 42 | `approval.request {session_id, tool, action, reason}` |
| Wire in | `src/crates/regent-deacon/src/application/dispatcher/session_ops.rs:284-303` | `approval.respond {session_id, approved, feedback?}` → `resolve_approval` |
| Decision type | `regent-tools` `ApprovalDecision` | `Approve` / `Deny` / `DenyWithFeedback(String)` |
| Posture gate | `.../session_ctx/approval.rs:21` | auto-approve deliberately **excludes** `ask_user`: "auto means skip permission prompts, not answer the agent's questions with a blanket yes" |
| Handler (gateway) | `src/crates/regent-gateway/src/application/approval.rs:76` | `ChatApprovalHandler` — posts the question into the chat, waits for a text reply via `ApprovalRouter`, with per-tool grace coalescing |

`ask_user` is registered for **code sessions only** (`register_ask_user_tool` doc
comment: "chat has the human in the loop").

### 1.2 How each surface answers today

- **CLI** — `features/chat/domain/chatPort.ts:20` `respondApproval(approved, feedback?)`.
  `ChatView.tsx:85-89` routes any non-affirmative reply as `feedback`, so a free-text
  answer to `ask_user` works. `StatusLine.tsx:89` shows an "awaiting approval" line.
  Rendering is a plain text prompt — there is no list, no navigation, no toggles.
- **Desktop app** — `features/chat/viewmodels/useChatSession.ts:320-325`
  `respondApproval(approved: boolean)`. **The `feedback` field is never sent.** The
  deacon accepts it; the app has no channel for it. So an `ask_user` question in the app
  can only be answered yes/no — a free-text answer is impossible today. Rendered from
  the `{kind: "approval"}` arm of `TranscriptItem`
  (`shared/kernel/transcript.ts:42-48`) via `Transcript.tsx` → `MessageRow.tsx`.
- **Platforms** — text only. `ChatApprovalHandler` writes the question as a chat message
  and parses the reply.

### 1.3 Current limitations (the actual gap)

1. **No structure.** `ask_user` carries two strings. Options exist only as prose inside
   `context`, so every surface renders a paragraph and every answer is free text the
   model must re-interpret — precisely the fragile plain-text path to avoid.
2. **No multi-select, no ranking, no typed confirm.**
3. **The app cannot send a free-text answer at all** (1.2).
4. **One question at a time.** `SessionEntry.approval_pending` is a single
   `Option<ApprovalTx>` — a second request overwrites the first, so the first waiter
   never resolves and times out at 120s. Sequential questions ("1 of 3") do not exist.
5. **Nothing survives a restart.** The `oneshot` lives in process memory only; a deacon
   restart or app reload loses the pending question with no record in the store.
6. **Auto-approving surfaces answer "yes" to everything** — the voice deacon runs
   `REGENT_AUTO_APPROVE`, and `env_auto_approver()` returns `AllowAll`, which does not
   have the `ask_user` exemption that `ConfigGatedApprover` has. `ask_user.rs` documents
   this as an accepted degenerate ("ponytail:" comment).
7. **Telegram has no buttons.** `telegram/wire.rs:30` `send_payload` is
   `{chat_id, text}` — no `reply_markup`; `parse_updates` reads `message.text` only and
   ignores `callback_query` entirely.

### 1.4 Images today

- **Markdown images already render.** `shared/ui/Markdown.tsx:97` maps `img` →
  `ZoomableImage` (`shared/ui/markdown/ZoomableImage.tsx`), which is an inline `<img>`
  plus a click-to-open lightbox with Esc-to-close. So `![](https://…)` in model output
  already appears as a picture.
- **What does not work:**
  - a **local path** (`C:\…\shot.png`, `$REGENT_HOME/artifacts/…`) — the CSP
    (`src-tauri/tauri.conf.json:27`) allows `img-src 'self' data: blob: https:` and has
    **no `asset:` scheme**, and the webview has no fs access, so a local file silently
    shows a broken image;
  - the **user's own uploaded image** — `stageAttachments` (`data/eventDetails.ts:83`)
    uploads via `attachment.put` and the transcript shows the **file name as a chip**
    (`transcript.ts:15-20`), never a thumbnail;
  - **no loading state and no error state** — `ZoomableImage` has neither, so a slow or
    dead URL renders as a broken-image glyph;
  - **tool-produced images** (camera capture, `create_document` previews, artifacts)
    surface as paths in a result summary, not pictures.
- **The precedent to copy:** `dispatcher/artifacts_ops.rs` already solves exactly this —
  "the webview has no filesystem access, so `artifacts.get` inlines small text and
  images (base64 data URI)", capped at `MAX_IMAGE_BYTES` (5 MB), gated by the same
  canonicalized within-root check as `attachment.put`.
- **Remote-content consent precedent:** `markdown/EmbedCard.tsx` gates YouTube/OSM
  embeds behind a click. Remote `https:` images currently bypass any such gate.

---

## 2. Proposed architecture

### 2.1 Questionnaire flow

```
agent calls ask_user{questionnaire:{…}}          regent-tools/infra/ask_user.rs
        ↓
ApprovalHandler::request_structured(&Questionnaire)   NEW default method on the trait
        ↓                                              (default impl = render to text and
        ↓                                               call the existing request())
   ┌────┴─────────────────────────────┐
   │                                  │
deacon: RpcApprovalHandler       gateway: ChatApprovalHandler
   │ question.request notif           │ PlatformAdapter::ask_question()
   │ (falls back to approval.request  │ (default = numbered text + reply parse)
   │  when the client is old)         │
   ↓                                  ↓
CLI / Desktop render a real UI    Telegram inline keyboard · WeChat numbered text
   │                                  │ Discord components
   └────────────┬─────────────────────┘
                ↓
      QuestionnaireAnswer (typed)
                ↓
   question.respond  /  callback payload
                ↓
      oneshot resolves → run_turn resumes → tool returns JSON to the model
```

**Why extend the approval channel instead of building a second one:** the pause/resume
machinery — oneshot parking, timeout-denies, per-session registry, interrupt
interaction — is already correct and already wired into three surfaces. A parallel
system would duplicate all of it. **Why a new notification rather than stuffing JSON
into `reason`:** a shipped client renders `reason` verbatim, so a JSON blob would appear
as raw text in front of users, and the answer would come back as an untyped string.

**Compatibility:** `session.create` gains an optional `capabilities: ["questions"]`.
When absent, the deacon renders the questionnaire to numbered text and uses the existing
`approval.request` — so today's CLI and app keep working unchanged against a new deacon.

### 2.2 Image flow

```
image reference in a message/tool result/attachment
        ↓
classify (Desktop, pure fn): data: | blob: | https: | local path
        ↓                                   ↓                ↓
   render directly                    consent gate      image.get RPC  ← NEW
                                     (EmbedCard rule)   (mirrors artifacts.get:
                                                         within-root check, 5 MB cap,
                                                         base64 data URI)
        └───────────────────────────────┬───────────────────┘
                                        ↓
                          <RegentImage> — loading · error · alt · lightbox
                                        ↓
                      ZoomableImage (existing lightbox, unchanged)
```

---

## 3. Data models

### 3.1 Rust — `src/crates/regent-kernel/src/contracts/questionnaire.rs` (new)

Kernel, because regent-tools, regent-deacon and regent-gateway all need it and the
kernel is the only crate all three already depend on.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    SingleSelect,
    MultiSelect,
    Text,
    Confirm,
    Rank,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Question {
    pub id: String,
    pub prompt: String,
    /// Very short chip label shown beside the question ("Auth method", "Scope").
    /// Taken from the reference implementation, where it earns its place in the
    /// card header — see §11.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub kind: QuestionKind,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    /// Offer a "Something else" free-text row alongside the options.
    /// DEFAULTS TO TRUE: the reference injects this row unconditionally and
    /// instructs the model never to author its own "Other" option, which is
    /// what stops a model from burning one of its four option slots on it.
    #[serde(default = "default_true")]
    pub allow_custom: bool,
    /// A skippable question resolves to `Skipped`; a required one re-asks once.
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Questionnaire {
    pub id: String,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Answer {
    /// Option ids, in the order the user chose them — which is also the ranking
    /// for `Rank`, so one variant covers select and rank.
    Selected { option_ids: Vec<String> },
    Text { text: String },
    Confirmed { yes: bool },
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestionnaireAnswer {
    pub questionnaire_id: String,
    /// One entry per answered question, keyed by `Question.id`.
    pub answers: Vec<(String, Answer)>,
    /// True when the user dismissed the whole card rather than answering.
    #[serde(default)]
    pub cancelled: bool,
}
```

**Validation** (`questionnaire::validate`, same file): non-empty `id`, non-empty
`prompt`, **1–5 questions**, **2–6 options each**, option ids unique within a question,
`SingleSelect`/`MultiSelect`/`Rank` require ≥ 2 options, `Confirm` and `Text` require
zero. A questionnaire that fails validation never reaches a surface — the tool returns a
`tool_error_json` naming the violation so the model can correct itself.

The caps are deliberately tight. The reference implementation enforces 1–4 questions and
2–4 options, and that bound is doing real work: a card with nine options is a menu, it
does not fit a Telegram inline keyboard or a terminal window, and it is a sign the model
should have asked a different question. Regent allows one more of each because `Rank`
over three items is not worth asking; beyond six, `validate` rejects.

### 3.2 Wire (JSON-RPC, additive)

```jsonc
// notification, deacon → client
{"method": "question.request", "params": {
   "session_id": "…", "questionnaire": { "id": "q_1", "questions": [ … ] }}}

// request, client → deacon
{"method": "question.respond", "params": {
   "session_id": "…", "answer": { "questionnaire_id": "q_1",
     "answers": [["q_1_0", {"kind": "selected", "option_ids": ["a"]}]],
     "cancelled": false }}}
```

### 3.3 TypeScript — mirrored, hand-copied, parity-tested

`src/regent-cli/src/features/chat/domain/questionnaire.ts` and
`src/regent-app/Desktop/shared/kernel/questionnaire.ts` carry the same shapes.
Two hand-copies of a Rust type is exactly the drift the repo has been bitten by
(`scripts/tests/verify-key-catalog.py` exists for the same reason), so Task 12 adds
`scripts/tests/verify-questionnaire-schema.py` to the `parity` CI job.

### 3.4 Desktop transcript additions — `shared/kernel/transcript.ts`

```ts
// TranscriptItem gains:
| {
    readonly kind: 'question';
    readonly questionnaire: Questionnaire;
    readonly answered?: QuestionnaireAnswer;
  }
// ChatEvent gains:
| { readonly type: 'question'; readonly questionnaire: Questionnaire }
| { readonly type: 'question-resolved'; readonly answer: QuestionnaireAnswer }
```

### 3.5 Image content

```ts
// Desktop shared/kernel/imageRef.ts (new)
export type ImageRef =
  | { readonly kind: 'inline'; readonly src: string }   // data: / blob:
  | { readonly kind: 'remote'; readonly src: string }   // https:
  | { readonly kind: 'local'; readonly path: string };  // fetched via image.get
```

---

## 4. CLI implementation

| File | Change | Responsibility |
|---|---|---|
| `src/regent-cli/src/features/chat/domain/questionnaire.ts` | create | Types (3.3) + `nextUnanswered()` + `applyAnswer()` — pure, unit-tested |
| `src/regent-cli/src/shared/ui/components/SelectList.tsx` | move | Promote from `features/setup/presentation/SelectList.tsx` (already a windowed arrow-key list with `SelectRow {label, hint}`); re-export from the old path so the wizard is untouched |
| `src/regent-cli/src/shared/ui/components/QuestionCard.tsx` | create | Renders one `Question`: numbered rows, `[✓]` toggles for multi-select, `❯` cursor, "Something else" row, footer hints. Render-only — parent owns `useInput`, matching `SelectList`'s existing contract |
| `src/regent-cli/src/features/chat/domain/useSelectionState.ts` | create | Pure selection state: cursor index, toggled set, rank order. No Ink import, so it unit-tests without a terminal |
| `src/regent-cli/src/features/chat/presentation/components/QuestionPrompt.tsx` | create | Owns `useInput` and nothing else: ↑↓ navigate, Space toggle, Enter submit, digit shortcuts, `e` for custom text, Esc skip/cancel. Delegates all state to `useSelectionState`; escalates to `MessageInput` for free text |
| `src/regent-cli/src/features/chat/presentation/ChatView.tsx` | modify | New `phase === "questioning"` branch that renders `QuestionPrompt` instead of the plain approval path (keeps `approving` untouched) |
| `src/regent-cli/src/features/chat/domain/transcript.ts` | modify | `questioning` phase + pending questionnaire in state |
| `src/regent-cli/src/features/chat/domain/chatPort.ts` | modify | Add `respondQuestion(answer: QuestionnaireAnswer)` |
| `src/regent-cli/src/features/chat/data/rpcChatAdapter.ts` | modify | Implement it as `question.respond` |
| `src/regent-cli/src/app/presentation/useBootstrap.ts` | modify | Send `capabilities: ["questions"]` on `session.create` |

**Non-TTY fallback:** `MessageInput` already only mounts when interactive. `QuestionPrompt`
checks `process.stdin.isTTY`; when false it prints the numbered question and reads one
line, mapping `1`/`1,3`/free text onto the same `QuestionnaireAnswer`. This is the same
path a piped `regent ask` uses, so scripts and CI keep working.

---

## 5. Regent App implementation

| File | Change | Responsibility |
|---|---|---|
| `Desktop/shared/kernel/questionnaire.ts` | create | Types + pure helpers (3.3) |
| `Desktop/shared/kernel/transcript.ts` | modify | `question` item + two events (3.4) |
| `Desktop/features/chat/presentation/question/QuestionCard.tsx` | create | The card: title, `1 of 3` progress, option rows, Skip, close (✕). ≤ 200 lines |
| `Desktop/features/chat/presentation/question/OptionRow.tsx` | create | One row: number chip, label, description, selected/checked state, hover/focus ring |
| `Desktop/features/chat/presentation/question/CustomInputRow.tsx` | create | "Something else" → inline textarea, Enter submits |
| `Desktop/features/chat/presentation/question/useQuestionKeys.ts` | create | ↑↓/1-9/Enter/Space/Esc, focus trap while open |
| `Desktop/shared/ui/Transcript.tsx` | modify | Route `kind: 'question'` to `QuestionCard`; add `onQuestion` alongside the existing `onApproval` |
| `Desktop/features/chat/viewmodels/useChatSession.ts` | modify | Handle `question.request`; add `respondQuestion`; **also add the missing `feedback` argument to `respondApproval`** (§1.2 defect) |
| `Desktop/shared/i18n/en/chat.ts` | modify | All new strings — no hardcoded copy in components |

**Design language:** tokens only (`bg-hover`, `--shadow-elev`, `palette` classes) — the
repo forbids raw colors in components (`Markdown.tsx` header). The card sits in the
transcript flow like the existing approval row, not as a modal overlay, so scrollback
history stays readable and a second question can follow the first.

**Accessibility:** `role="radiogroup"` / `role="group"` with `aria-checked` rows,
`aria-live="polite"` on the progress counter, visible focus rings, ≥ 44px touch targets,
Esc only when the question is skippable.

### 5.1 Images

| File | Change | Responsibility |
|---|---|---|
| `Desktop/shared/kernel/imageRef.ts` | create | `classifyImageSrc()` — pure, unit-tested |
| `Desktop/shared/ui/markdown/RegentImage.tsx` | create | Loading skeleton → image → error card with the URL as a copyable fallback; wraps `ZoomableImage` for the lightbox |
| `Desktop/shared/ui/markdown/ZoomableImage.tsx` | modify | Accept a `status` prop; keep the existing lightbox behaviour byte-for-byte |
| `Desktop/shared/ui/Markdown.tsx:97` | modify | `img` → `RegentImage` instead of `ZoomableImage` |
| `Desktop/shared/ui/MessageRow.tsx` | modify | User attachments that are images render as thumbnails instead of name-only chips |
| `Desktop/features/chat/data/imageCache.ts` | create | In-memory `Map<path, dataUri>`, LRU-capped at 20 entries, cleared on session switch |
| `src/crates/regent-deacon/src/application/dispatcher/image_ops.rs` | create | `image.get {path}` — mirrors `artifacts_get`: canonicalized within-root check, 5 MB cap, returns `{mime, data_uri}` |
| `src/crates/regent-deacon/src/application/dispatcher/mod.rs` | modify | Register `"image.get"` |

**Roots allowed for `image.get`:** `$REGENT_HOME/attachments`, `$REGENT_HOME/artifacts`,
and the session's workspace when one is bound. Anything else is `-32602` — same posture
as `attachment_within_root`.

---

## 6. Platform integration

### 6.1 Telegram (first)

- **Adapter:** `regent-gateway/src/infra/platforms/telegram.rs` (+ `telegram/wire.rs`).
- **Native UI available:** inline keyboards (`reply_markup.inline_keyboard`) answered by
  `callback_query` updates, plus `answerCallbackQuery` to clear the client spinner.
- **Confirmed limitation:** `send_payload` today is `{chat_id, text}` with no
  `reply_markup`, and `parse_updates` reads `message.text` only — `callback_query`
  updates are dropped on the floor.
- **Useful fact:** `callback_query` **is** in Telegram's default `allowed_updates`, so
  buttons need no change to the `getUpdates` call (unlike `message_reaction`, which is
  excluded by default).
- **Changes:** `send_payload` takes an optional `&Questionnaire` and emits one button per
  option with `callback_data = "<questionnaire_id>:<question_id>:<option_id>"` (Telegram
  caps `callback_data` at **64 bytes** — ids are generated short, and the builder
  truncates deterministically with a hash suffix); `parse_updates` gains a
  `callback_query` arm producing a new `MessageEvent`-sibling; multi-select renders as
  toggle buttons plus a "Submit" button, editing the message in place via
  `editMessageReplyMarkup`; free text falls back to "reply with your answer".
- **Fallback:** if `sendMessage` returns `400` for the markup, resend as numbered text.

### 6.2 WeChat (second)

- **Adapters:** `platforms/wechat/mod.rs` (Official Account, Customer Service API) and
  `platforms/wecom.rs` (Work/Enterprise).
- **Limitation — the honest one:** the Official Account customer-service message API
  supports `msgtype: text | image | voice | video | news`; there is **no per-message
  button**. Persistent menus exist but are account-level, not per-question. Inbound,
  `wechat/mod.rs:141` accepts `MsgType == text` only.
- **So WeChat renders the questionnaire as numbered text** and parses `1`, `1,3`, `yes`
  or free text from the reply. This is a capability limit, not a shortcut.
- **WeCom is better:** `template_card` supports button lists; implement buttons there and
  keep numbered text for the Official Account.

### 6.3 Discord (third)

- **Two separate surfaces already exist:** `regent-gateway/src/infra/platforms/discord.rs`
  (WebSocket Gateway adapter, real chat) and
  `regent-deacon/src/infra/discord_interactions.rs` (Ed25519-signed slash-command
  webhook that acks with a deferred type-5 response).
- **Native UI:** message components — buttons and string select menus.
- **Limitation:** component interactions arrive as `MESSAGE_COMPONENT` (type 3) on the
  **interactions webhook**, not the gateway socket, so answering a question posted by the
  socket adapter requires the webhook path to be configured and to route the callback
  back into the same `ApprovalRouter`. Document this as a deployment prerequisite.
- **Fallback:** numbered text on the socket adapter when no interactions endpoint is set.

### 6.4 The rest

`PlatformAdapter` (`regent-gateway/src/domain/contracts.rs:12`) gains:

```rust
async fn ask_question(&self, _chat_id: &str, _q: &Questionnaire)
    -> Result<QuestionSupport, GatewayError> { Ok(QuestionSupport::Unsupported) }
```

defaulting to `Unsupported` exactly like `send_file` does today. `ChatApprovalHandler`
renders numbered text for every `Unsupported` adapter, so Slack, Teams, LINE, Feishu,
Messenger, WhatsApp, Mattermost, Google Chat, Twilio and email keep working with zero
per-adapter code. Slack (Block Kit), Feishu (interactive cards) and Teams (Adaptive
Cards) are the natural next three.

---

## 7. Testing strategy

**Unit**
- Rust: `questionnaire::validate` accept/reject table; `Answer` serde round-trip;
  Telegram `send_payload` with/without a questionnaire; `callback_data` truncation.
- CLI: `nextUnanswered`/`applyAnswer`; key handling (space toggles, enter submits).
- Desktop: `classifyImageSrc`; transcript reducer for `question` + `question-resolved`.

**Integration**
- Deacon: `question.request` → `question.respond` resolves the oneshot and the turn
  resumes (mirrors the existing approval round-trip test in
  `regent-deacon/tests/deacon_basics/`).
- Deacon: an old client (no `capabilities`) gets `approval.request` with numbered text.
- Gateway: `gateway_behavior/approvals.rs` gains a questionnaire case with `MockAdapter`.

**Component/UI**
- Desktop: `bun test` on the reducer + a render test per question kind.
- CLI: `ink-testing-library` render of `QuestionCard` per kind.

**Platform adapters** — mocked payloads: a Telegram `callback_query` body parses to the
right answer; a WeChat numbered reply maps to the right option; a Discord type-3
interaction routes to the router.

**Regression** — existing approval flow untouched: `askRun.test.ts` and the deacon
approval tests must pass unmodified.

**Manual** — the CLI and app checklists in §10, Phase 10.

---

## 8. Migration / compatibility

- **No schema migration.** Questions are transient; nothing is written to `state.db` in
  phases 1–9.
- **Stored history is unchanged.** A questionnaire that has been answered is rendered
  into the transcript as the existing `user`/`notice` items, so `session.history` on an
  old deacon still replays correctly.
- **Old client + new deacon:** no `capabilities` → text fallback (§2.1).
- **New client + old deacon:** `question.request` never arrives; the app keeps handling
  `approval.request`. Both arms stay in the reducer permanently.
- **Restart behaviour (documented, phase-9 decision):** a pending question is lost on
  deacon restart and the turn ends with the 120s timeout-deny, exactly as a pending
  approval does today. Persisting pending questionnaires in `state.db` is deliberately
  **out of scope** for the first release — it changes turn-resume semantics for
  approvals too, and should be one ADR of its own.

---

## 9. Security and reliability

| Risk | Mitigation |
|---|---|
| Malformed questionnaire from the model | `validate()` before it leaves the tool; error JSON back to the model |
| Unknown option id in an answer | Deacon rejects with `-32602`; the surface re-renders |
| Duplicate submission (double click, retried callback) | `questionnaire_id` matched against the parked oneshot; a second respond is a no-op `{"resolved": false}` |
| Stale question after restart | Timeout-deny at 120s, same as approvals |
| Unauthorized answer on a platform | The existing `AuthPolicy` check on `user_key` already gates every inbound event; the callback arm reuses it (a button press from a non-paired user is dropped) |
| Unexpected platform callback | Unknown `callback_data` → `answerCallbackQuery` with "this question has expired" |
| Prompt injection via option labels | Labels are rendered as **text**, never markdown, in both surfaces |
| Huge/broken images | 5 MB cap in `image.get`; `RegentImage` error state; `loading="lazy"` |
| Path traversal via `image.get` | Canonicalized within-root check reused from `attachment_within_root` |
| Untrusted remote images | `https:` only (CSP already blocks `http:`); a settings toggle to require a click before loading remote images, following the `EmbedCard` precedent |
| HTML injection | React escapes by default; no `dangerouslySetInnerHTML` anywhere in the new components |
| Race: answer arrives as the turn is interrupted | `turn.interrupt` clears `approval_pending`; the respond then no-ops |
| Auto-approve answering questions | Fix the `env_auto_approver()` gap so `AllowAll` also exempts `ask_user`/questions (§1.3 item 6) |

---

## 10. Implementation phases

Each phase is independently shippable and leaves the tree green.

- **Phase 1 — Schema.** `regent-kernel` types + `validate` + serde tests. TS mirrors +
  the parity script wired into the `parity` CI job.
- **Phase 2 — Runtime lifecycle.** `ApprovalHandler::request_structured` default method;
  `ask_user` gains an optional `questionnaire` argument; deacon `question.request` /
  `question.respond`; `capabilities` on `session.create`; text fallback. Old-client
  integration test.
- **Phase 3 — CLI UI.** Promote `SelectList`, add `QuestionCard` + `QuestionPrompt`,
  wire the `questioning` phase, non-TTY fallback.
- **Phase 4 — App UI.** `QuestionCard` + rows + keyboard hook + transcript wiring; fix
  `respondApproval` to carry `feedback`.
- **Phase 5 — App images.** `image.get`, `classifyImageSrc`, `RegentImage`, attachment
  thumbnails, loading/error states.
- **Phase 6 — Telegram.** `reply_markup`, `callback_query`, `answerCallbackQuery`,
  multi-select via message edit, mocked-payload tests.
- **Phase 7 — WeChat.** Numbered-text renderer + reply parser; WeCom `template_card`
  buttons.
- **Phase 8 — Discord.** Components on the socket adapter + type-3 routing through the
  interactions webhook.
- **Phase 9 — Remaining adapters.** Confirm the `Unsupported` default renders sensibly
  on Slack/Teams/LINE/Feishu; ADR for pending-question persistence.
- **Phase 10 — Regression + docs.** Full gate run, manual CLI and app checklists,
  changelog entry, ADR for the questionnaire contract.

### Manual checklist (Phase 10)

**CLI:** single select · multi-select with Space · free text via "Something else" ·
confirm · Esc skip · Ctrl-C cancel · digit shortcuts · turn resumes after answering ·
non-TTY piped run.

**App:** card appears in the transcript · click and keyboard selection · multi-select ·
custom input · Skip · ✕ close · `1 of 3` advances · answer reaches the agent and the turn
continues · markdown image renders · local attachment renders · multiple images in one
message · broken URL shows the error card · lightbox opens and Esc closes · plain text
messages unchanged.

---

## 11. What the reference implementation taught us

Studied at `D:\1-1@k\1-1Claudecode\ClaudeCodesrc\src` — specifically
`tools/AskUserQuestionTool/AskUserQuestionTool.tsx` (264 lines), its `prompt.ts`,
`components/permissions/AskUserQuestionPermissionRequest/`, and the `components/CustomSelect/`
module. Concepts adapted, nothing copied.

**Adopted**

- **`header`** — a short chip label per question. It is what makes the card read as a
  form field rather than a paragraph, and it is visible in the supplied screenshots.
- **The free-text row is automatic, not opt-in.** The reference's tool description tells
  the model outright: *"There should be no 'Other' option, that will be provided
  automatically."* That single instruction stops a model from spending one of its few
  option slots re-inventing the escape hatch. Hence `allow_custom` defaults to true.
- **Hard, low caps on questions and options** (§3.1 validation).
- **Separate state / navigation / input / render.** The reference splits
  `use-select-state`, `use-multi-select-state`, `use-select-navigation`,
  `use-select-input` and the render components. Regent's CLI mirrors that split via
  `useSelectionState.ts` + `QuestionPrompt.tsx` + `QuestionCard.tsx`.
- **`description` under each option label** — the screenshots show it carrying the real
  decision content ("Uniquement les forfaits mobiles pour commencer").

**Deliberately not adopted**

- **Answers keyed by question text, as a flat `string → string` map**, with multi-select
  answers comma-joined. It makes the wire self-describing, but it means every consumer
  re-parses a string, two questions worded identically collide, and a comma inside a
  label corrupts the answer. Regent keys by `Question.id` and returns typed `Answer`
  variants instead.
- **`multiSelect: boolean` instead of a kind enum.** It is genuinely simpler, but it
  cannot express `Rank`, and `Confirm` degrades to an untyped two-option question. The
  brief asks for both as first-class, so Regent keeps `QuestionKind`.
- **Option `preview` with a side-by-side layout.** A good idea and out of scope for the
  first release; it is additive to `QuestionOption` whenever it is wanted.
- **The file sizes.** `select.tsx` is 669 lines and `use-select-navigation.ts` is 555 —
  both would fail this repo's ≤250-line convention. Take the decomposition, not the
  proportions.

---

## Appendix — files touched, at a glance

**Create (13):** `regent-kernel/src/contracts/questionnaire.rs` ·
`regent-deacon/src/application/dispatcher/question_ops.rs` ·
`regent-deacon/src/application/dispatcher/image_ops.rs` ·
`regent-cli/.../domain/questionnaire.ts` · `regent-cli/.../components/QuestionCard.tsx` ·
`regent-cli/.../components/QuestionPrompt.tsx` ·
`Desktop/shared/kernel/questionnaire.ts` · `Desktop/shared/kernel/imageRef.ts` ·
`Desktop/.../question/QuestionCard.tsx` · `Desktop/.../question/OptionRow.tsx` ·
`Desktop/.../question/CustomInputRow.tsx` · `Desktop/.../question/useQuestionKeys.ts` ·
`Desktop/shared/ui/markdown/RegentImage.tsx`

**Modify (16):** `regent-tools/src/infra/ask_user.rs` ·
`regent-tools/src/domain/contracts.rs` (trait default) ·
`regent-deacon/.../hooks.rs` · `.../session_ops.rs` · `.../dispatcher/mod.rs` ·
`.../session_ctx/approval.rs` · `regent-gateway/src/domain/contracts.rs` ·
`regent-gateway/src/application/approval.rs` ·
`regent-gateway/.../platforms/telegram.rs` + `telegram/wire.rs` ·
`.../platforms/wechat/mod.rs` · `.../platforms/wecom.rs` · `.../platforms/discord.rs` ·
`regent-cli/.../ChatView.tsx` · `regent-cli/.../transcript.ts` ·
`regent-cli/.../chatPort.ts` + `rpcChatAdapter.ts` ·
`Desktop/shared/kernel/transcript.ts` · `Desktop/shared/ui/Transcript.tsx` ·
`Desktop/shared/ui/MessageRow.tsx` · `Desktop/shared/ui/Markdown.tsx` ·
`Desktop/.../useChatSession.ts` · `Desktop/shared/i18n/en/chat.ts`
