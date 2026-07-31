# Approval and sandbox boundary map

**Date:** 2026-07-31
**Plan item:** A.5 of [`regent-cli-superiority-plan-2026-07-30.md`](../research/regent-cli-superiority-plan-2026-07-30.md)
**Method:** source reading, every claim below carries a `file:line`. The registered-tool
list is additionally pinned by `src/crates/regent-tools/tests/approval_coverage.rs`, which
fails if a tool is added, renamed or removed without updating its recorded posture.

**Scope, stated honestly.** This maps the `core_catalog()` tool set, the composition roots
that build catalogs, and the switches that change either. It does **not** yet enumerate
every process spawn in the codebase (§3.1 lists the ones found, not a proof of
completeness), the tools the gateway adds, or dynamically registered MCP tools beyond
noting that they are gated. The plan asked for "every execution entry point"; this is a
first pass that is accurate about what it covers rather than a complete one.

> Why this exists: the CLI evaluation verified that `ApprovalDecision` is fail-closed **by
> type** — no `Default`, no bool coercion, deny is the fallthrough. It never verified
> **coverage**: which execution paths consult the gate at all. Until that is written down,
> "safe by construction" is a claim about one enum.

---

## 1. The gate

`ApprovalHandler::request(tool, action, reason) -> ApprovalDecision`
— `src/crates/regent-tools/src/domain/contracts.rs:48`.

Named postures:

| Posture | Behaviour | Where |
|---|---|---|
| `DenyAll` | denies everything | `contracts.rs:60` |
| `VoiceScopedApprover` | denies everything (a call cannot display a prompt) | `contracts.rs:72` |
| `AllowAll` | approves everything | `contracts.rs:86` |
| `RpcApprovalHandler` | prompts the connected client and blocks on the answer | `regent-deacon/.../session_manager/hooks.rs:26` |
| `ConfigGatedApprover` | `AllowAll` while `tools.auto_approve` is on, else prompts | `.../session_ctx/approval.rs:13` |

`ConfigGatedApprover` re-reads the flag **per request** (`approval.rs:21`), so a live
`config.set tools.auto_approve` changes open sessions, not just new ones. `ask_user` is
excluded from auto-approval by name (`approval.rs:21`) — auto means "skip permission
prompts", not "answer the user's questions with a blanket yes".

## 2. Which registered tools reach the gate

Verified call sites of `ctx.approval.request`:

| Tool | Gated when | Where |
|---|---|---|
| `terminal` | **only** when `detect_dangerous_command` matches one of 12 regexes | `infra/terminal.rs:107`, patterns at `domain/guard.rs:8` |
| `control_app` | every action | `infra/control_app.rs:86` |
| `computer_use` | mutating actions (read-only screen reads are not gated) | `infra/computer_use/mod.rs:110` |
| MCP tools | every call | `infra/mcp_tools.rs:196` |
| any tool | when a permission rule matches with `action: Ask` | `application/catalog.rs:144` |
| `ask_user` | uses the gate as the question channel, not as a guard | `infra/ask_user.rs:52` |

**Registered, mutating, and holding no tool-local approval call** — the finding this
document exists for:

`write_file`, `file_edit`, `apply_patch`, `create_document`, `image_generation` (which does
write a PNG to disk — `infra/image_generation.rs:75`), `open_url`, `play`. Verified as an
absence: `infra/files.rs`, `infra/file_edit.rs` and `infra/apply_patch/` contain zero
occurrences of `approval`.

Precision matters here, because "never gated" would be false. These tools **can** reach the
handler — through the catalog-level permission machinery, when a rule matches with
`action: Ask` (`application/catalog.rs:144`). What makes that a theoretical route rather
than a live one is that `ToolContext` constructs `permission_rules` **empty**
(`domain/entities.rs:64` and `:83`), and nothing in the deacon's session build populates
them. So the accurate statement is:

> With no permission rules configured — which is the default — a file write, patch or
> document render reaches no approval handler at all. A destructive-looking shell command
> does.

That may well be the intended trade: `regent code` provides plan → verify → **revert** for
repo edits, and prompting on every write would make the agent unusable. It is written down
here because it was written down nowhere, and because §8.2 of the plan specifies a headless
`--yes` whose stated meaning is "allow the agent to write files and run commands" — which
implies a default that denies them.

**Consequence for headless.** The read-only default cannot be delivered by the approval
handler, because with default rules the write tools never consult it. It has to be a
permission **rule set** (`PermissionAction::Deny`, `domain/permissions.rs:11`). And that
rule set has to deny more than the write tools: `terminal` in its entirety, not merely the
12 dangerous patterns — an ordinary `sh -c 'echo x > f'` matches none of them and is not
gated (asserted by `an_ordinary_terminal_command_is_not_gated_and_does_reach_the_backend`
in the invariant test). Hooks, `control_app`, `open_url`, `play` and delegated/MCP tools
need the same treatment.

### 2.1 Gated but unreachable

`move_file`, `copy_file` and `delete_file` do call the gate (`infra/file_ops.rs:78,106,128`)
and are **registered by no catalog**. They appear only in the `tools.deferred` default list
(`regent-deacon/src/domain/config/runtime.rs:278-280`), which names strings, not tools. They
are dead code today. Deleting them or registering them is a decision, not an accident to
leave standing.

## 3. Execution paths outside the tool catalog

| Path | Approval posture | Sandbox posture |
|---|---|---|
| Chat session tools | `ConfigGatedApprover` / env posture | `core_catalog_from_env()` — `REGENT_SANDBOX` enforced |
| `delegate_task` workers | inherit the parent `ToolContext` | inherit — `catalogs.rs:42` uses `core_catalog_from_env()` |
| `code_task`, `background_task`, `explore` | in-process, via `self_ref` | as above (`catalogs.rs:80-121`) |
| Cron jobs | `DenyAll` (`bin/regent-deacon/main.rs`) | `core_catalog_from_env()` — enforced (fixed 2026-07-31, §4) |
| Kanban worker + reviewer | `DenyAll` (`board_dispatch.rs`) | `core_catalog_from_env()` — enforced; refuses to start if the catalog is denied |
| `regent mcp serve` | `DenyAll` (`bin/regent-mcp.rs`) | `core_catalog_from_env()` — enforced |
| `tools.hook_tool_start` / `hook_tool_complete` | **outside the gate entirely** — user-configured shell spawned at both dispatch seams (`session_manager/mod.rs:163`, `infra/shell_hook.rs`) | host process |
| Gateway (Slack/WhatsApp/…) | its own approver with a grace window (`regent-gateway/src/application/approval.rs:191`) | `core_catalog_from_env()` |

### 3.1 Order of operations inside a dispatch

From `application/catalog.rs:135-165`, in order:

1. permission rules evaluate — `Deny` returns immediately, `Ask` calls the handler and a
   denial returns immediately (`:135-161`);
2. **`hook_tool_start` fires** (`:162`);
3. the tool executes — and any *tool-local* approval call, such as `terminal`'s
   dangerous-command gate, happens here, inside step 3.

So a `terminal` command the user then denies has **already run the start hook**. Hooks
cannot veto a dispatch (`infra/shell_hook.rs:1-6`), but "cannot veto" is not "has no
effect": the hook is arbitrary user-configured shell that runs before the human is asked.
Any headless or read-only posture has to account for hooks explicitly rather than treating
them as observation.

### 3.2 Process spawns outside the terminal tool

Tools that spawn OS processes without going through a `TerminalBackend`, and therefore
without any sandbox backend selection: `camera`, `computer_use` (cua + powershell),
`control_app`, `create_document` (preview and renderer), `key_tool/env_file`, `no_window`,
`open_url`, `play`, `reveal`, `shell_hook`. Found by grepping `process::Command` under
`regent-tools/src/infra/`; treat as the list that exists, not a proof that no others do.

## 4. Sandbox: does opt-in ever silently fall back?

`REGENT_SANDBOX=1` combined with the host `local` backend is a **hard error**, not a
fallback — `infra/sandbox.rs:139`:

```rust
if sandbox_on && backend_describe == "local" {
    return Err(RegentError::Config("REGENT_SANDBOX is set but ... 'local'"))
}
```

But that check lives in `terminal_backend_from_env()` (`infra/backends.rs:123`), which is
reached **only** through `core_catalog_from_env()`. `core_catalog()` constructs
`LocalBackend` directly (`application/registry.rs:23`) and never calls `enforce_backend`.

**Therefore:** on those three paths `REGENT_SANDBOX` is never *interpreted*. Calling it a
"silent fallback" would be imprecise — they do not attempt sandbox selection and then give
up; they never attempt it. The user-visible result is the same and is the problem: with
`REGENT_SANDBOX=1` set, cron jobs, kanban workers and `regent mcp serve` execute through
`LocalBackend` on the host, and produce neither a sandbox nor the configuration error that
the same flag produces everywhere else.

They are `DenyAll`, so a *dangerous-pattern* command is refused — but every command that
does not match one of the 12 regexes runs unprompted on the host.

This is the one item here that reads as a defect rather than an undocumented trade-off.

> **FIXED 2026-07-31.** All three now build their catalog with
> `core_catalog_from_env()`, so `REGENT_SANDBOX` means the same thing everywhere. The
> board dispatcher fails **closed** — if the catalog is refused it logs and does not start,
> rather than falling back to a host shell. Pinned by
> `sandbox_opt_in_refuses_the_host_backend_rather_than_falling_back` and
> `the_plain_constructor_is_the_one_without_enforcement` in
> `regent-tools/tests/approval_coverage.rs`.
>
> The remaining structural improvement is to move `enforce_backend` into
> `core_catalog_with_terminal` so no constructor *can* skip it; today the discipline is
> enforced by a test rather than by the type system.

## 5. Security-boundary controls

Split by what they actually control — the plan called these all "bypass flags", and lumping
sandbox selection in with approval bypass is how a posture report ends up misleading.
"Complete" is not claimed: this is what a search for the documented names plus
`std::env::var` in the approval and sandbox modules found.

**Approval bypasses** — change whether a human is asked:

| Switch | Effect | Where |
|---|---|---|
| `tools.auto_approve` (config) | `AllowAll` for everything except `ask_user`; re-read per request, so it live-reloads into open sessions | `session_ctx/approval.rs:21`, `wiring.rs:29` |
| `REGENT_AUTO_APPROVE` | read when a **session is built**; every session built while it is set gets `AllowAll` (not re-read per request like the config flag) | `session_ctx/approval.rs:30,48` |
| permission rule with `action: Allow` | suppresses a rule-driven `Ask` for matching calls | `domain/permissions.rs:11` |
| gateway grace window | re-approval is skipped inside the window after one approval | `regent-gateway/src/application/approval.rs:204` |
| `REGENT_VOICE` + `REGENT_AUTO_APPROVE` | `VoiceScopedApprover` (deny-all) unless full control | `session_ctx/approval.rs:33` |
| `REGENT_VOICE_FULL_CONTROL` | promotes the above to `AllowAll` | `session_ctx/approval.rs:33` |

**Isolation controls** — change *where* execution happens, not whether it is approved:

| Switch | Effect | Where |
|---|---|---|
| `REGENT_SANDBOX` | forbids the `local` backend — only on the `from_env` path (§4) | `infra/sandbox.rs:139` |
| `REGENT_UNSAFE_NO_SANDBOX` | widens the filesystem jail | `session_ctx.rs:96` |
| `REGENT_TERMINAL_BACKEND` | selects local / docker / ssh / sandbox | `infra/backends.rs:121` |

**Surface controls** — change what exists to be approved:

| Switch | Effect | Where |
|---|---|---|
| `REGENT_COMPUTER_USE` | registers the desktop-control tool at all | `registry.rs:110` |
| `tools.hook_tool_*` | user shell at both dispatch seams, ungated, and before the tool's own approval (§3.1) | `session_manager/mod.rs:163` |

`REGENT_ALLOW_ALL` is **not** an approval bypass. It exists only in the gateway's user
allow-list (`regent-gateway/src/domain/auth.rs:102`) and controls *who may talk to the
bot*, not what the agent may do. The plan listed it under approval bypasses; that was wrong.

## 6. What this licenses, and what it does not

Standing checks now in CI (`regent-tools/tests/approval_coverage.rs`), at two very
different strengths — conflating them would be the fastest route to false confidence:

**Inventory tripwire.** Every tool `core_catalog()` registers has a *recorded* approval
posture; a new or renamed tool fails the build until someone classifies it. It proves set
equality between the catalog and a hand-maintained table. It does **not** prove the table
is truthful: a dangerous new tool filed under `UNGATED` passes.

**Behavioural.** A destructive `terminal` command reaches the handler and, when denied,
never reaches the terminal backend — asserted against a recording backend rather than
against the returned string, because a denial *message* is not evidence that nothing ran.
A companion test asserts an ordinary command still does reach the backend, so the first
cannot pass by dispatch being broken.

Not covered, and deliberately not attempted here (Phase F work):

* No test proves an *ungated* tool cannot mutate — that requires running each tool.
* No sandbox enforcement test asserts from **inside** a container (F.2), and none asserts
  that the three §4 constructors sandbox at all.
* No egress test, and no threat model to test against (F.3).
* Hook execution has no invariant at all, including the §3.1 ordering.

## 7. Recommended follow-ups, in order

1. ~~**Route cron, kanban and `regent mcp serve` through `core_catalog_from_env()`.**~~
   **Done 2026-07-31** (§4). Left open: making it structurally impossible rather than
   test-enforced.
2. Decide `move_file`/`copy_file`/`delete_file`: register them or delete them (§2.1).
3. Before headless `--yes` ships, define the read-only default as a **permission rule set**
   (§2), remembering that it must deny `terminal` wholesale rather than relying on the
   dangerous-pattern list, and must cover hooks and delegated/MCP dispatch.
4. Give hooks a posture — at minimum a `doctor`/`security` line saying they are configured
   and that they run outside the gate. They are observe-only and cannot veto a dispatch
   (`infra/shell_hook.rs:1-6`), which bounds the risk: a hook is an extra execution path,
   not a way around the gate. It is still arbitrary shell the config can turn on, and
   nothing today tells a user it is armed.
