# Handoff - 2026-08-09 - Regent Refining Audit and Release Checkpoint

This handoff records the Regent refining audit requested across self-learning,
Butler, CLI/TUI, provider setup, API Keys, token accounting, documentation,
installer delivery, and the proposed multi-file workspace editor.

The completed audit work was committed first, as requested:

- Branch: `fix/new-fix-verification-2026-07-24`
- Commit: `2f4e2272530fd0781a5cafd44d45c85aea820999`
- Subject: `feat: harden Regent audit flows and accounting`
- Commit size: 87 files, 2,240 insertions, 407 deletions
- Push/PR: not performed
- This handoff file was intentionally created after that commit and is therefore
  not part of `2f4e227`.

## Executive status

The main audit changes are checkpointed. Token reporting, local-provider setup,
API-key boundaries, Butler voice-tool residency, one-turn provider routing,
self-learning write restrictions, Desktop settings separation, and current docs
are implemented in the commit.

The branch is not ready to push as a clean release handoff yet. Three product
items remain:

1. The autonomous learning reviewer can still be started by two Regent deacon
   processes for the same parent-session range. Live logs and a new regression
   prove the duplicate call. Durable cross-process leasing is partially drafted
   in the working tree but is not complete and was deliberately excluded from
   the commit.
2. `/with <provider>/<model> ...` resolves its route before the normal per-turn
   `.env` refresh. A key saved immediately before that command may be reported
   missing until an ordinary turn reloads credentials.
3. VS Code-style workspace editor tabs are not implemented. The design needs
   explicit approval before changing draft/close behavior.

The full workspace is also not green in the current dirty working tree because
the new lease regressions are intentionally red. The committed audit itself was
covered by the successful focused and product suites listed below before the
lease work was started.

## Scope-by-scope outcome

| Requested area | Outcome | State |
|---|---|---|
| 1. Autonomous, safe self-learning | Reviewer writes are now deterministically append-only and its transcript is explicitly untrusted. Duplicate cross-process review ownership remains open. | Partially complete |
| 2. Butler tool delay and diagrams | Reproduced the voice-session environment mismatch; direct media/app-control tools now remain resident. Existing diagram tests passed, so no speculative renderer rewrite was made. | Complete for confirmed defect |
| 3. CLI and TUI behavior | CLI was compiled and exercised through the full suite. Local-provider error handling now distinguishes configuration from connectivity failures. | Complete, with `/with` refresh edge below |
| 4. API-key provider logic | Local providers are key-optional and grouped separately. Provider setup now registers a routable provider/default model. Unsupported image/video/music rows were removed. | Complete |
| 5. Duplicate Messaging settings | Messaging platforms were removed from API Keys and remain under Gateway. | Complete |
| 6. Token efficiency and accuracy | Provider-reported usage is now accounted across ordinary turns, compaction, wrap-up, title, Distiller, provider tests, and MoM calls. Context estimate and billed/reported tokens are separated. | Complete for accounting; cost tuning recommended |
| 7. Skills/tools page delay | The user confirmed the page is now fast and effectively instant. No extra code was changed for an issue that was no longer reproducible. | Closed by user confirmation |
| 8. Per-task provider/model | `/with <provider>/<model> <task>` routes one turn only and fails closed. Immediate post-key-save refresh remains open. | Mostly complete |
| 9. Documentation consistency | Canonical config/routing, context/token language, self-learning boundaries, and stale counts were corrected. | Complete |
| 10. Installer audit | Windows command/GUI installer checks passed and a real NSIS installer was built and hashed. POSIX harness could not run in this Windows environment. | Windows complete; POSIX blocked by environment |
| Workspace editor tabs | A bounded v1 design is documented below. No implementation was made without approval. | Pending design approval |

## 1. Self-learning: autonomy and safety

### What is committed

The learning loop remains autonomous. No approval gate was inserted before it
learns. The security boundary is instead enforced at the available write tools:

- User-profile learning is append-only.
- Memory learning is add-only.
- Skill learning may create a new skill.
- The reviewer cannot replace or remove trusted memory.
- The reviewer cannot rewrite Regent's persona.
- The reviewer cannot patch or archive an existing skill.
- The review prompt labels the conversation snapshot as untrusted evidence,
  preventing user transcript text from becoming reviewer instructions.

Relevant areas include:

- `src/crates/regent-skills/src/application/prompts.rs`
- `src/crates/regent-tools/src/infra/memory_tools.rs`
- `src/crates/regent-tools/src/infra/persona_tool.rs`
- `src/crates/regent-tools/src/infra/skill_tools.rs`
- `src/crates/regent-agent/tests/learning_loop/review_targets.rs`

The adversarial learning-loop regression passed before the later lease red tests
were added.

### Confirmed remaining defect: duplicate cross-process reviews

Live logs showed two deacon processes reviewing the same parent session and
message range. A 2026-08-07 sample showed the same session reviewed at 22:44 and
22:46. The recent log sample contained 14 turns, 19 provider calls, and 5 review
completions; the duplicate was a coordination defect, not evidence that the
model itself intentionally repeated work.

The current uncommitted regression reproduces the defect with two independent
`Store` and `Agent` instances sharing one SQLite database:

```text
review_gate::two_process_views_review_the_same_parent_range_only_once
left: 2
right: 1
one durable claim must fence the second process before a model call
```

Current focused command:

```powershell
cargo test -p regent-agent --test learning_loop `
  two_process_views_review_the_same_parent_range_only_once -- --nocapture
```

Result on 2026-08-09: 0 passed, 1 failed, 8 filtered out.

### Required production fix

Use a durable, token-fenced lease owned by the shared store, not an in-memory
mutex. The intended protocol is:

1. In one `BEGIN IMMEDIATE` transaction, compare the durable reviewed cursor,
   active lease, expiry, and requested target range.
2. Return one of `Acquired(lease)`, `Busy`, or `Covered`.
3. Acquire before creating the reviewer session or calling a model.
4. Renew the lease while the reviewer is active.
5. Finish by atomically advancing the reviewed cursor and clearing the lease,
   but only when the opaque token still owns it.
6. Release on known failure; after a process crash, allow safe retry only after
   expiry.
7. A stale owner must not renew, release, or finish a successor's lease.

Draft work exists but is incomplete in the uncommitted working tree. The store
regression currently fails because the draft query references columns that have
not yet been added:

```text
no such column: review_claim_token
```

Focused store command:

```powershell
cargo test -p regent-store --test store_roundtrip review_claims -- --nocapture
```

Result on 2026-08-09: 0 passed, 2 failed, 7 filtered out.

Keep schema version 11 for the committed accounting migration. The claim fields
can be nullable reconciled session columns unless a separate numbered migration
is genuinely required:

- `review_claim_token`
- `review_claim_start`
- `review_claim_end`
- `review_claim_until`

Do not call this fixed until both focused suites are green and a crash/expiry
case is covered.

## 2. Butler delay, tool use, and diagrams

The confirmed Butler problem was an environment-contract mismatch. The app uses
`REGENT_VOICE`, while the relevant warm/resident-tool logic did not consistently
read that same signal. The shared contract is now centralized and voice
sessions keep `play` and `control_app` resident, avoiding a load-tools retry for
common spoken media/app commands.

The focused Butler regression passed:

```text
voice_sessions_keep_direct_media_and_app_control_tools_resident
1 passed, 0 failed
```

Settings also exposes `speech.call.fast_model` as a provider/model picker with
an explicit inherit-main-model state. This makes model latency a configurable
factor without pretending every delay originates in the model.

Diagram rendering was tested first and the existing path passed. No renderer
or diagram-display change was made because the originally suspected problem was
not present in the tested build.

Interpretation:

- Direct tool delay: confirmed tool-residency/environment issue and fixed.
- General response latency: may still be influenced by the selected provider,
  model size, network, or speech pipeline.
- Diagram display: no confirmed current defect; no speculative fix shipped.

## 3. CLI/TUI and error handling

### Real client-side evidence

The CLI was freshly compiled and its full suite passed: 205/205 tests, with
typecheck and lint clean. Coverage included compiled shell behavior, setup
integration, and real PDF/PNG handling.

A scratch LM Studio flow was tested from the client side with no local server
running:

```text
regent setup --provider lmstudio --model local-model \
  --base-url http://localhost:1234
```

Observed behavior after the fix:

- Setup exits successfully and says the API key is optional.
- No `.env` file is created for an unprotected local server.
- `providers.lmstudio` is written with its base URL and model.
- `agents_defaults.primary` points to the new route.
- Legacy `model.*` fields remain only for compatibility.
- `regent doctor --json` exits with failure because the configured endpoint is
  unreachable, while its configuration/deacon checks remain independently OK.
- The failure names `http://localhost:1234/v1/models` and tells the user to
  start LM Studio or correct `providers.lmstudio.base_url`.
- `regent providers test lmstudio` recognizes the configured route and reports
  a network connection error. It no longer says the just-created provider is
  unknown.

This is unambiguous: setup validity, key requirements, and endpoint reachability
are separate results.

## 4. Providers and API Keys

### Local/self-hosted providers

These providers are now treated as key-optional by default and grouped in their
own collapsible Local & self-hosted section:

- Ollama
- LM Studio
- llama.cpp
- vLLM
- LiteLLM proxy

An explicit key remains supported for a protected local endpoint. A generic
cloud `REGENT_API_KEY` is not silently attached to a local provider unless the
user intentionally supplies a key.

Removing the local key field entirely would have broken authenticated local
gateways. The implemented behavior is the better compromise: key entry is
optional, visually subordinate, and available only when the deployment needs
it.

### Cloud, speech, vision, and generation categories

The settings catalog was aligned with adapters Regent actually ships:

- LLM provider keys remain available.
- Speech provider configuration remains available.
- Vision/video-analysis keys remain available where implemented.
- The generic image-generation key remains available.
- Rows that implied unsupported Stability/Runway/Suno-style image, video, or
  music generation were removed.

The UI no longer promises a provider integration that has no runtime adapter.

### API-key RPC security boundary

`env.set` no longer accepts arbitrary credential-shaped environment variables.
It now accepts exact variables from Regent's provider, integration, and speech
catalogs plus canonical numbered provider-key slots 2 through 8.

Confirmed rejected examples:

- `DATABASE_URL`
- `ATTACKER_API_KEY`
- `AWS_SECRET_ACCESS_KEY`
- `REGENT_API_KEY_02`

The focused environment suite passed 9/9, and the full deacon run that included
this change passed 262 library, 6 binary, and 73 integration tests, with one
measurement ignored.

## 5. Messaging settings ownership

Messaging platforms now render only on the Gateway page. The API Keys page no
longer duplicates those rows. A messaging-only payload produces no visible API
key rows and displays the empty state instead.

Focused Desktop view-model test: 3 passed, 0 failed. Desktop typecheck passed.

## 6. Token accounting, context accuracy, and efficiency

### Committed accounting model

Provider-reported usage now flows through one ledger for:

- Ordinary chat model calls
- Context-compaction summaries
- Budget-exhaustion wrap-up calls
- Session-title generation
- Distiller calls
- Provider connectivity/model tests
- Mixture-of-Models proposers
- Mixture-of-Models aggregator

Successful provider calls that omit usage no longer appear as genuine zero-token
calls. They increment `unreported_usage_calls`. Upgraded pre-v11 databases also
expose `legacy_usage_unverified`, because exact historical omissions cannot be
reconstructed honestly.

The Desktop uses session-aware atomic snapshots so concurrent sessions do not
pair one session's token result with another session's context. It distinguishes:

- Context estimate: the estimated size of the next request/context window.
- Last request: the latest provider-reported input/output usage.
- Turn spend: all provider-reported calls attributed to that turn.
- Lifetime/insights totals: persisted session plus non-session usage.

This prevents the app from presenting an estimate as a billed fact.

### Historical live totals observed during the audit

The live database snapshot contained:

| Metric | Observed total |
|---|---:|
| Sessions | 1,265 |
| Messages | 12,108 |
| Turns | 1,933 |
| Successful turns | 1,282 |
| Failed turns | 651 |
| API calls | 4,862 |
| Input tokens | 101,578,126 |
| Output tokens | 1,272,767 |
| Total tokens | 102,850,893 |

These are historical aggregate data, not a clean benchmark of commit `2f4e227`.
The old ledger did not cover every call, and the app does not currently attach
provider price data, so this audit cannot claim a reliable monetary spend.

### Safe efficiency priorities

In order of value without degrading answer quality:

1. Finish the durable review lease. Duplicate reviews waste a complete reviewer
   call and are pure overhead.
2. Configure `model.review` to a smaller capable review model instead of the
   current very large NVIDIA Nemotron 550B route when quality tests permit.
3. Configure `speech.call.fast_model` for low-latency Butler commands while
   retaining the main model for complex work.
4. Use the new counters to measure unreported calls and per-turn amplification
   before changing context, compaction, or review thresholds.
5. Do not reduce context or output budgets globally based only on historical
   totals; compare task quality and call count first.

## 7. Skills and tools page

The user reported that Skills and Tools now loads fast and instantly. Because
the delay was no longer present, no additional change was made. If it regresses,
capture a Desktop performance trace and deacon RPC timing before adding caching
or eager loading.

## 8. One-turn provider/model selection

The supported syntax is:

```text
/with <provider>/<model> <task>
```

Example:

```text
/with openrouter/anthropic/claude-sonnet-4.6 review this design
```

Behavior:

- Resolves an exact configured provider/model route before the turn.
- Routes only that task.
- Restores the session's normal provider afterward.
- Fails clearly for an unknown provider or missing credential.
- Never silently falls back to the default provider.
- Preserves vendor-prefixed model IDs internally.
- Compacts repeated provider prefixes only in CLI display text.

### Known refresh edge

The route is currently resolved before the normal per-turn `.env` refresh. If a
user saves a provider key and immediately sends `/with`, that turn can report a
missing key even though the key was just saved. An ordinary turn refreshes the
credentials, after which the route works.

Fix this in `src/crates/regent-deacon/src/application/session_manager/run.rs` by
refreshing task-relevant credentials before resolving the explicit route. Keep
the fail-closed behavior and add a regression that saves a key, sends `/with`
immediately, and proves the selected provider receives that same turn.

## 9. Documentation audit

Committed documentation corrections include:

- Canonical `providers.<name>` registry examples.
- Canonical `agents_defaults.primary` routing examples.
- Clear statement that legacy `model.*` fields are compatibility fields.
- Local provider key optionality.
- `/with` syntax.
- Desktop context estimate versus reported token semantics.
- Autonomous self-learning's append-only write boundary.
- Archive-not-delete curator behavior.
- Removal of stale hard-coded workspace crate and ADR counts.
- API Keys versus Gateway ownership.
- Current installer commands.

Updated files:

- `README.md`
- `docs/README.md`
- `docs/QUICKSTART.md`
- `docs/PROJECT-OVERVIEW.md`
- `docs/development/voice-and-api-calls.md`
- `docs/changelogs/CHANGELOG.md`

## 10. Installer and release audit

### Verified checks

- Version/protocol parity passed: app version `0.1.1`, protocol version `7`.
- Release Python suite: 10/10 passed.
- Windows `install.ps1` harness: 30/30 passed.
- Installer Rust tests: 9/9 passed.
- Installer clippy: clean.
- Installer frontend production build: passed.
- Desktop Tauri no-bundle build: passed.
- NSIS GUI installer build: passed.

Correct NSIS command from `src/regent-app/Installer`:

```powershell
bun run tauri build --bundles nsis
```

Do not insert an extra `--`; `bun run tauri build -- --bundles nsis` forwards
the option incorrectly to Cargo.

### Built Windows artifact

```text
D:\1-1@k\@ServeAI\Regent\src\regent-app\Installer\src-tauri\target\release\bundle\nsis\Regent Setup_0.1.1_x64-setup.exe
```

- Size: 70,873,139 bytes
- SHA-256: `F8736ED1E0FF31C7B044A7EACA646F667AEC75A2AE08B19A016565ECABC9E20C`
- Existence and hash rechecked after commit on 2026-08-09.

The Desktop no-bundle executable was also produced at:

```text
src/regent-app/Desktop/src-tauri/target/release/regent-desktop.exe
```

### Environment-limited checks

- The POSIX installer harness could not run because this machine had no usable
  `sh`, and WSL startup was denied by the environment.
- External NVIDIA/OpenRouter probes returned sandbox network errors. An
  unsandboxed retry was not approved, so provider availability was not judged
  from those results.

## Verification matrix

| Area | Result | Notes |
|---|---|---|
| CLI compile/typecheck/lint | Passed | Fresh client build |
| CLI tests | 205/205 passed | Includes setup integration and real PDF/PNG paths |
| Desktop typecheck/build | Passed | Production build completed |
| Desktop tests | 317/317 passed | Before uncommitted lease work |
| Desktop Tauri no-bundle | Passed | Executable produced |
| Agent doom-loop | 3/3 passed | Final focused rerun |
| Agent learning loop | 8/8 passed | Before adding the new cross-process red regression |
| Deacon full tests | Passed | 262 lib + 6 binary + 73 integration; 1 measurement ignored |
| Store baseline | Passed | 68 checks before the lease red tests |
| Release Python | 10/10 passed | Release tooling |
| Windows installer harness | 30/30 passed | Client-side script behavior |
| Installer Rust | 9/9 passed | Plus clippy clean |
| NSIS package | Passed | Artifact and hash above |
| Full Cargo workspace | Environment failure | Stopped in `regent-code` clean-tree test because Git could not read the global ignore file |
| Current store lease regression | Failed intentionally | 2 tests fail: lease columns absent |
| Current agent duplicate-review regression | Failed intentionally | Reviewer called twice instead of once |

### Full-workspace environment failure

`cargo test --workspace --jobs 1` compiled for more than ten minutes and reached
`regent-code`. There, 21 tests passed and one failed:

```text
infra::git_ops::tests::committing_a_clean_tree_fails_with_a_clear_message
```

The temporary Git repository emitted only:

```text
warning: unable to access 'C:\Users\Ralph Lacanlale/.config/git/ignore': Permission denied
```

The assertion therefore could not see Git's normal `nothing to commit` text.
This is an environment/global-Git-config access failure, not evidence that the
Regent code under audit changed clean-tree behavior. A permitted environment
should rerun the full workspace suite.

## Workspace editor tabs: proposed v1

No editor-tabs code was committed. The current workspace editor owns one active
file/draft/revision, so adding a tab strip only at the view layer would lose
drafts and create asynchronous file-switch races.

Recommended v1 behavior, pending owner approval:

- VS Code-style tabs for multiple opened files.
- Tabs retained per chat for the current app process.
- Each tab preserves its own unsaved draft and revision.
- Closing a dirty tab prompts Save, Don't Save, or Cancel.
- Closing the folder/workspace offers Save All for dirty tabs.
- Only the active tab is exposed to chat context.
- No split panes, preview/pinned mode, drag reorder, or restart persistence in
  v1.

Recommended architecture:

1. Add a domain tab model keyed by normalized absolute file path.
2. Extract a `useEditorTabs` view model from the existing workspace state.
3. Store per-tab loading/error/draft/base-revision/dirty state.
4. Cache CodeMirror `EditorState` per path so cursor, selection, undo, and scroll
   survive tab switches.
5. Guard asynchronous file reads with a request/version token so a slow open of
   file A cannot overwrite the state after the user switches to file B.
6. Extract tab strip, editor pane, and dirty-close dialog components instead of
   growing the already large `WorkspacePanel`.
7. Keep filesystem save/conflict rules in the workspace feature boundary; do
   not duplicate them in presentation components.
8. Add tests for open/dedupe, tab switching, dirty-close choices, Save All,
   external revision conflict, per-chat retention, and the A-to-B async race.

Approval question for the next owner session: accept the bounded v1 above, or
change dirty-close and persistence behavior before implementation.

## Current uncommitted working tree

The following task-related lease work remains outside commit `2f4e227`:

- `src/crates/regent-agent/tests/learning_loop/review_gate.rs`
- `src/crates/regent-store/src/domain/entities.rs`
- `src/crates/regent-store/src/infra/mod.rs`
- `src/crates/regent-store/src/infra/review_claims.rs`
- `src/crates/regent-store/src/lib.rs`
- `src/crates/regent-store/tests/store_roundtrip/main.rs`
- `src/crates/regent-store/tests/store_roundtrip/review_claims.rs`

They form an incomplete draft: store APIs and types exist locally, but schema
columns and agent acquisition/heartbeat integration are not complete. Do not
commit them as a fix in their current state.

Known unrelated/user-owned changes deliberately preserved outside the audit
commit:

- `.claude/`
- `src/crates/regent-deacon/tests/deacon_basics/jobs.rs`
- `src/regent-app/Desktop/features/workspace/presentation/LogView.tsx`

This handoff file will also appear as untracked until explicitly committed.

## Recommended continuation order

1. Complete the durable review lease using the red store and agent tests.
2. Rerun all store and learning-loop tests; then run the full agent and deacon
   suites.
3. Fix `/with` credential refresh ordering with a save-key-then-route regression.
4. Obtain explicit editor-tabs v1 approval, then implement it with per-tab
   state and async race protection.
5. Run CLI and Desktop typecheck/lint/tests/build from a client-side profile.
6. Rerun `cargo test --workspace --jobs 1` in an environment where Git can read
   its global excludes file.
7. Run the POSIX installer harness on Linux or a functioning WSL environment.
8. Rebuild and re-hash installers only after the remaining code changes land.
9. Inspect the final staged diff, update this handoff/changelog if outcomes
   change, commit, and only then consider push/PR.

## Resume commands

From the repository root:

```powershell
cargo test -p regent-store --test store_roundtrip review_claims -- --nocapture
cargo test -p regent-agent --test learning_loop `
  two_process_views_review_the_same_parent_range_only_once -- --nocapture
cargo test -p regent-agent --test learning_loop
cargo test -p regent-deacon --jobs 1
cargo test --workspace --jobs 1
```

CLI:

```powershell
Set-Location src/regent-cli
bun run typecheck
bun run lint
bun test
bun run compile
```

Desktop:

```powershell
Set-Location src/regent-app/Desktop
bun run typecheck
bun test
bun run build
bun run tauri build --no-bundle
```

Installer:

```powershell
Set-Location src/regent-app/Installer
bun run build
cargo test --manifest-path src-tauri/Cargo.toml
bun run tauri build --bundles nsis
```

## Handoff rule

Do not describe Regent as push-ready while either durable duplicate-review
ownership or the immediate `/with` credential refresh edge remains unresolved.
For every follow-up, keep the same discipline used in this audit: reproduce at
the real client boundary, write a failing regression, implement the minimum
root-cause fix, and report passed, failed, and environment-blocked checks
separately.
