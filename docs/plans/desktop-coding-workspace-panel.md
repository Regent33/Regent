# Desktop coding workspace panel — file editor + git

## Context

Desktop's main chat page has no way to see or edit the files a coding session
touches, or to commit/push them. The user asked for a VSCode-like panel
(collapsed by default) on the chat page: a file tree + editor, plus Commit /
Commit+Push / Push buttons, functional end to end, with Ctrl/Cmd+S save.

Investigation before this plan found the request wasn't just "add a panel" —
Desktop's deacon process pins its cwd to `$REGENT_HOME/artifacts` (a sandbox;
added after a code task once scaffolded a project inside a real checkout), so
Desktop coding sessions today never touch a user's real repo. The CLI, by
contrast, inherits its real launch directory already. There is no file
read/write RPC for an arbitrary working tree (only the artifacts-sandboxed
`artifacts.*` methods), and no git add/commit/push anywhere (only
snapshot/restore for the code-task revert-on-fail safety net).

The user confirmed two decisions:
1. **Workspace scope: both.** Per session, the user should be able to work in
   the existing sandbox (today's default) OR open a real project folder
   (like VSCode's Open Folder) — not a global switch.
2. **Editor: Monaco** (`@monaco-editor/react`), the production-grade, "the way
   VSCode does it" choice, lazy-loaded so it doesn't bloat the main bundle
   (mirrors the existing `ButlerView` lazy-chunk precedent).

## Architecture

### Per-session workspace override (Rust)

`SessionManager` has one global `cwd: PathBuf` set once at boot. Per-session
state already lives in `SessionEntry` (`session_manager/hooks.rs`). Add an
`Option<PathBuf>` workspace override there, resolved once at session creation
and immutable after (no live mid-session workspace switching — starting a new
session is how you switch projects, same as VSCode's "Open Folder" replacing
the window's project).

- `create_session_keyed(...)` gains a trailing `workspace: Option<PathBuf>` —
  every existing call site (CLI sessions, background tasks, keyed platform
  sessions) passes `None`, preserving today's behavior exactly.
- `tool_context()` resolves the base `cwd` via a small pure helper
  (`resolve_cwd(default, override) -> PathBuf`) instead of always reading
  `self.cwd`.
- **A workspace session is sandboxed to its own root.** Today local Desktop
  sessions run the *unsandboxed* branch (`ToolContext::new`, `sandbox: None`),
  and `ToolContext::resolve` returns any absolute path unchecked
  (`regent-tools/src/domain/entities.rs:109-110`) — harmless only because cwd
  is a disposable `$REGENT_HOME/artifacts` folder. The moment the root becomes
  the user's real repo, that same unchecked path is one hallucination or
  prompt-injection away from their home directory, dotfiles, and sibling
  projects. So the sandbox condition becomes
  `external || sandbox_enabled() || workspace.is_some()`, reusing the existing
  `new_sandboxed(root, root, approval).allow_subtree(artifacts)` path already
  used for external sessions. Because `code_plan`/`code_start`'s child
  sessions go through this same `create_session_keyed` → `tool_context()`
  path, one conditional covers the sessions that do the actual editing too.
- `code_task`'s tool already receives `ctx: &ToolContext` per call (currently
  unused as `_ctx`) — `ctx.cwd` is exactly the calling session's resolved
  workspace. `code_plan`/`code_start` (`session_manager/code.rs`) take an
  explicit `workspace: Option<PathBuf>` parameter instead of reading the
  manager's global `self.cwd`, so a code task run from a session with an open
  folder edits/verifies/reverts against THAT folder.
- The CLI's raw `code.plan`/`code.start` RPC surface (`dispatcher/code_ops.rs`)
  keeps passing `None` — zero behavior change for `regent code`.
- Workspace is persisted per session in the store (one new nullable column)
  so resuming a session after an app restart re-opens the same folder.

### New RPC surface

- `session.create` gains an optional `workspace` param (validated: must exist,
  must be a directory; rejected together with `conversation_key`).
- `workspace.get` / `workspace.tree` / `workspace.read` / `workspace.write` —
  session-scoped, path-containment-checked (reusing the existing
  `attachment_within_root` helper), 5 MB read/write cap (hard reject, never
  truncate — a truncated read-then-save would truncate the file on disk).
  `workspace.read` uses **strict** UTF-8 decoding (not the lossy pattern
  `artifacts.get` uses for read-only preview) and reports `binary: true`
  rather than risking silent corruption on save-back.
- **`workspace.read`/`write` carry an optimistic-concurrency token** (file
  mtime + size, or a content hash). `read` returns it; `write` must echo it
  back and is **rejected** if the file changed on disk since that read. This
  closes a real lost-update hole that "disable while busy" does NOT cover: an
  editor buffer opened *before* a code task runs still holds pre-turn content,
  and the user can save it moments *after* busy flips false, silently
  clobbering the agent's just-verified edit. The panel also refreshes the open
  file on `tool.complete` for `code_task` so the buffer doesn't sit stale.
  This must be in the contract from slice 5 — retrofitting it later reworks
  the Rust tests and the TS mappers.
- `git.status` / `git.diff` / `git.commit` / `git.push` — session-scoped,
  shell out to the real `git` CLI (not libgit2 — shelling out gets the user's
  own credential helper/SSH agent for free, matching the existing
  `GitCheckpoint` convention in `regent-code`). Live in a new
  `regent-code::infra::git_ops` module; the deacon's dispatcher methods are
  thin wrappers. `git.push` runs detached (like `code.plan`/`code.start`) and
  needs the long-timeout bucket on the Tauri bridge (`request_timeout`).

### Desktop UI

New `features/workspace/` (viewmodels + presentation, mirrors the existing
`features/artifacts/` shape): a lazy per-directory file tree (unlike
artifacts' flat 2-level list, this needs arbitrary depth), an Open-Folder
button (new `tauri-plugin-dialog`, scoped to directory-picking only — actual
file I/O still goes through the deacon RPC bridge, preserving the "no fs
plugin" security invariant already documented in
`src-tauri/capabilities/default.json`), a Monaco editor lazy-loaded only when
a file is opened, Ctrl/Cmd+S wired via the same pure-predicate-plus-listener
pattern as `devtoolsGuard.ts`, and Commit/Commit+Push/Push buttons — Commit
never confirms (local, reversible), Push and Commit+Push both route through
the existing `ConfirmDialog` (remote/shared-state ops always confirm, project
rule). All three disabled while the session is busy, so a running `code_task`
and a manual edit+save can't race the same tree.

`useChatSession`'s lazy `session.create` (fired on first submit) is
generalized into `ensureSession(workspace?)` so the panel's Open-Folder flow
can create the session too, if the user opens a folder before ever sending a
message — first caller wins, same single-flight guard as today's
double-submit race protection.

## Scope boundaries (deliberately out for v1)

- No per-hunk staging, no branch-switching UI, no auto `--set-upstream`
  (push assumes an upstream is already configured).
- No `.gitignore`-aware tree filtering — a small hardcoded ignore list
  (`.git`, `node_modules`, `target`, `dist`, `out`, `.next`, `.turbo`).
- No new-file / rename / delete-from-tree — v1 is open-and-edit-existing-files
  only (`attachment_within_root` requires the file to already exist; note
  `ToolContext`'s own `contained()` helper at `entities.rs:127-148` DOES handle
  not-yet-existing paths, so reuse that rather than growing a second
  containment implementation if v2 adds file creation).
- Path containment is canonicalize-then-open, so it is not atomic — a symlink
  swapped in during that window could redirect a write (TOCTOU). Accepted for
  a single-user desktop app; don't describe the check as airtight.
- No live mid-session workspace switching, no concurrent multi-repo
  `code_task` execution (already impossible today — one code task runs
  process-wide at a time).
- Only TS/JSON/CSS/HTML get real language-service IntelliSense from bundled
  Monaco workers; every other language gets syntax highlighting only, no LSP.

## Build order (test-first, dependency order)

1. Pure `resolve_cwd` helper + test.
2. Store persistence: `workspace` column + `set_session_workspace`/
   `session_workspace` + round-trip tests.
3. Thread `workspace` through `create_session_keyed` / `tool_context` /
   `SessionEntry` / `make_entry`; update all 7 call sites; add
   `workspace_root`/`workspace_is_default` accessors. Includes the
   sandbox-when-`workspace.is_some()` conditional, with a test asserting a
   workspace session's `ToolContext` is sandboxed and rejects an absolute path
   outside its root (the security guarantee, not just plumbing).
4. `regent-code::infra::git_ops` (status/diff/commit/push), moving the
   existing `code.rs::diff_of` into it as `git_diff`; real-subprocess tests
   with `tempfile::tempdir()` + `git init` (same convention as
   `checkpoint.rs`'s existing tests).
5. `dispatcher/workspace_ops.rs` (tree/read/write) + tests mirroring
   `artifacts_ops_tests.rs`, including the concurrency token: a write with a
   stale token is rejected, a write with the current token succeeds, and a
   non-UTF-8 file reports `binary` instead of decoding lossily.
6. `dispatcher/git_ops.rs` RPC methods + `session.create`'s workspace param
   + tests.
7. Desktop pure-function layer (`isSaveShortcut`, response mappers) + tests.
8. `useChatSession`'s `ensureSession(workspace?)` generalization.
9. UI wiring: `WorkspacePanel`/`FileTree`/`GitToolbar`, mount into
   `ChatView.tsx`, add the Tauri dialog plugin (npm + Cargo + capability).
10. Monaco integration (lazy-loaded `CodeEditor.tsx`, Vite `?worker` imports
    for the editor/json/ts/css/html workers, `loader.config({monaco})` to
    kill the default CDN fetch) — last, highest-friction, least
    unit-testable piece.

## Verification

```
cargo test -p regent-store       # workspace column round-trip
cargo test -p regent-code        # git_ops + existing checkpoint/verify/diagnostics
cargo test -p regent-deacon      # workspace_ops_tests, session_ops workspace validation
cargo build                      # whole-workspace — catches any missed call site
bun test                         # isSaveShortcut + pure mapping helpers
bun run build                    # confirm Monaco lands in its own lazy chunk
```

Manual end-to-end: throwaway git repo + local bare remote, open it via the
panel before sending any chat message, edit + Ctrl/Cmd+S, confirm on disk,
Commit (no confirm), Push (confirm dialog, lands on the bare remote), Commit+
Push (one confirm, both land), run a `code_task` against the same opened
folder and confirm Save/Commit/Push disable while busy, restart the app and
resume the session to confirm the same folder re-opens, start a second
folder-less chat to confirm it still defaults to the sandbox, and attempt to
open a >5MB file and a binary file to confirm both are refused cleanly.
