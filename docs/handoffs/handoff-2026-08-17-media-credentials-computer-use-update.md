# Regent handoff — media credentials, dependable automation, and update status

Date: 2026-08-17  
Branch: `main`  
Starting commit: `d63b874680a7` (also `origin/main`)  
State: **paused by owner request; uncommitted and not pushed**

## Read this first

This work restores the missing image/video provider rows, makes `computer_use`
stay visible to the agent, adds a safe status-only `regent update`, and hardens
credential storage. It is **not ready to push yet**: the third hostile review
was stopped before a SHIP verdict and returned five unresolved findings listed
under [Open blockers](#open-blockers).

The requested media scope is **credential management only**. The rows securely
store provider credentials; they do not pretend Regent already ships a native
adapter for every listed provider. Regent currently has a generic image path and
does not yet have a native video-generation tool.

## Why the image and video rows disappeared

Commit `2f4e2272` intentionally removed the earlier Stability and Runway rows.
The old settings UI made the rows look like working native adapters, although
Regent did not ship those provider adapters. That avoided a misleading promise,
but it also removed the useful ability to securely save credentials.

This change restores the rows with an explicit note: **credential storage only**.
That preserves honest product boundaries while letting users prepare credentials
for generic adapters, skills, MCP servers, and future native integrations.

## What changed

### 1. Dedicated image and video sections in Desktop Settings

Settings → API Keys now has separate **Image generation providers** and
**Video generation providers** panels.

Image providers include Stability, Replicate, fal.ai, Leonardo, Ideogram,
Black Forest Labs, Recraft, Clipdrop, Segmind, DeepAI, Luma, Kling, and Haiper.

Video providers include Runway, Luma, Kling, Pika, Haiper, HeyGen, Synthesia,
D-ID, Tavus, and Vidu. Luma, Kling, and Haiper appear in both panels because one
provider account/key covers both product families; Regent stores one secret.

Higgsfield is deliberately absent: its published automation path uses
account-based CLI/MCP authentication rather than a public API key.

### 2. `computer_use` stays resident for automation

When `REGENT_COMPUTER_USE=1` enables the tool, `computer_use` is now part of the
light pinned set and cannot be removed by stale deferred-tool configuration.
The stable prompt already tells the model that it is the preferred GUI/browser
automation tool, so the agent should no longer claim it cannot control an open
browser and wait for the user to remind it.

When the runtime flag is off, no unavailable tool is advertised.

### 3. Safer CLI and agent credential management

- `regent keys set <NAME> --stdin` reads secrets from standard input so they do
  not appear in shell history or process arguments.
- `regent setup --key-stdin` provides the same protection during onboarding.
- Only an exact managed allowlist is accepted; runtime posture variables such as
  `REGENT_AUTO_APPROVE` cannot be written through the key command/tool.
- Canonical numbered slots `_2` through `_8` are supported.
- Rust and TypeScript writers share the same `.env.lock/` directory protocol.
- Writes use a secure temporary file, owner-only permissions, flush-to-disk, and
  atomic replacement.
- Duplicate assignments are normalized; removal deletes every assignment.
- Gateway, voice, setup, CLI keys, and the agent key tool use shared storage
  funnels rather than separate ad-hoc writers.

### 4. `regent update` now works as an honest Phase-0 command

`regent update` and `regent update --check` report:

- CLI version
- running deacon version
- latest version known by the deacon
- whether that result came from network/cache/disabled state
- last check timestamp

The command is status-only. It does **not** download, replace, or silently mutate
the installation. Mixed CLI/deacon versions, no cached check, and disabled checks
return an honest failure instead of saying the system is up to date.

### 5. Windows PTY shell resolution

The selected bare Windows shell is resolved through `where.exe` before ConPTY
starts it. This closes the CI/runtime mismatch where a Linux runner tried to run
`cmd /c ...` and where a shell name could be accepted without an executable path.

## File-by-file change map

Line numbers below refer to the current working tree on 2026-08-17. They may move
after formatting or later edits.

### Repository and documentation

| File | Current lines | Change and reason |
|---|---:|---|
| `.gitignore` | 42-46 | Ignores `docs/reference/` after it was removed from Git, preserving the local files without allowing accidental re-addition. |
| `docs/QUICKSTART.md` | 36-49, 117-126, 191-211 | Documents `regent update`, safe stdin setup, dedicated media credential panels, example image/video keys, and the credential-only boundary. |
| `docs/changelogs/CHANGELOG.md` | 3-48, 276-277 | Explains why the rows were removed/restored and records computer-use pinning, updater behavior, locking, numbered slots, secret-input hardening, and PTY resolution. |
| `docs/plans/cross-component-update-system.md` | 18-23 | Clarifies that Phase 0 reports cached update state and never downloads or replaces binaries/configuration. |
| `docs/reference/cli-guide.md` | deleted from Git (was 1-545) | Owner requested removal of the entire `docs/reference` folder from Git. File remains local and ignored. |
| `docs/reference/commands.md` | deleted from Git (was 1-72) | Same owner-requested untracking; local copy preserved. |
| `docs/reference/env-vars.md` | deleted from Git (was 1-162) | Same owner-requested untracking; local copy includes the media-variable draft and is preserved. |

### Rust — deacon, automation context, provider rows, and PTY

| File | Current lines | Change and reason |
|---|---:|---|
| `src/crates/regent-deacon/src/application/dispatcher/env_ops.rs` | 51-95 | Uses the shared managed catalog, includes cross-listed groups, and rejects variables outside the exact settable catalog. |
| `src/crates/regent-deacon/src/application/dispatcher/env_ops_tests.rs` | 142-196, 271-293 | Tests image/video rows, cross-listing, settable behavior, and rejection of arbitrary credential-shaped variables. |
| `src/crates/regent-deacon/src/application/dispatcher/status_ops.rs` | 39-45 | Changes the browser-control hint to the stdin-safe key command. |
| `src/crates/regent-deacon/src/application/pty/shell.rs` | 70-115 | Resolves a Windows shell to an executable path before launch. |
| `src/crates/regent-deacon/src/application/pty/tests/shell.rs` | 76-94 | Pins the resolved-shell behavior. |
| `src/crates/regent-deacon/src/application/session_manager/build.rs` | 76-78, 89-118, 199-209, 243-248 | Makes `computer_use` resident and light-pinned when available; removes stale deferral. |
| `src/crates/regent-deacon/src/application/session_manager/tests/build.rs` | 30-47, 75-79 | Tests the minimal light set and automation/media residency. |
| `src/crates/regent-deacon/tests/deacon_basics/tiering.rs` | 261-316 | Real integration test checks full/light catalogs with computer use enabled/disabled and stale config present. |

### Rust — managed key catalog and secure `.env` publication

| File | Current lines | Change and reason |
|---|---:|---|
| `src/crates/regent-tools/src/infra/browser.rs` | 3-9 | Replaces the unsafe command-line key example with an stdin example. |
| `src/crates/regent-tools/src/infra/key_tool/catalog.rs` | 1-236 | Defines exact media/provider groups, cross-listing, protected variables, managed keys, and canonical slots `_2.._8`. |
| `src/crates/regent-tools/src/infra/key_tool/env_file.rs` | 17-161, 243-278, 300-497 | Holds a cross-process lock across read/modify/publish, normalizes duplicates, reloads numbered credentials, protects temp files before writing secrets, flushes, and atomically replaces `.env`. |
| `src/crates/regent-tools/src/infra/key_tool/mod.rs` | 20-203, 245-280 | Enforces the catalog in agent key actions, validates values, lists numbered slots, and safely sets/deletes keys. |
| `src/crates/regent-tools/src/infra/key_tool/tests/env_file.rs` | 44-76, 101-219 | Tests numbered reload, ACL failure preservation, duplicate normalization/removal, concurrent writers, and Unicode-safe masking. |

### Desktop Settings

| File | Current lines | Change and reason |
|---|---:|---|
| `src/regent-app/Desktop/features/settings/presentation/ApiKeysSection.tsx` | 27-45 | Renders image/video headings and the credential-only explanatory note. |
| `src/regent-app/Desktop/features/settings/viewmodels/useApiKeys.ts` | 9-34 | Adds image/video to the visible API-key group order. |
| `src/regent-app/Desktop/features/settings/viewmodels/useApiKeys.test.ts` | 10-17 | Guards the dedicated group order. |
| `src/regent-app/Desktop/shared/i18n/en/settings.ts` | 124-129 | Beginner-friendly section names and honest storage/adapters wording. |

### CLI commands and update status

| File | Current lines | Change and reason |
|---|---:|---|
| `src/regent-cli/src/app/cli/router.ts` | 58, 190-191 | Routes the new `regent update` command. |
| `src/regent-cli/src/app/config/commands/ops.ts` | 39-56, 70 | Documents `--key-stdin`, `update`, and stdin-only key examples. |
| `src/regent-cli/src/features/keys/domain/keyCatalog.ts` | 1-162 (new) | TypeScript mirror of the exact managed catalog, groups, and `_2.._8` slot policy. |
| `src/regent-cli/src/features/keys/cli/keysCommand.ts` | 1-184 | Implements list/set/remove with stdin-only values, strict allowlist validation, numbered slots, and shared atomic storage. |
| `src/regent-cli/src/features/keys/cli/keysCommand.test.ts` | 1-142 (new) | Tests media keys, injection rejection, exact allowlist, duplicates, numbered slots, and five real concurrent CLI processes. |
| `src/regent-cli/src/features/update/cli/updateCommand.ts` | 1-81 (new) | Renders authoritative cached update status and handles mixed/disabled/unknown states honestly. |
| `src/regent-cli/src/features/update/cli/updateCommand.test.ts` | 1-96 (new) | Tests current, available, unknown, disabled, mixed-version, and invalid-usage cases. |
| `src/regent-cli/src/features/update/domain/notice.ts` | 20-43 | Carries check timestamp/source/note from the deacon response. |
| `src/regent-cli/src/features/update/domain/notice.test.ts` | 15-24 | Pins parsing of those status fields. |

### Shared TypeScript credential writer and all callers

| File | Current lines | Change and reason |
|---|---:|---|
| `src/regent-cli/src/shared/infrastructure/storage/dotenvLock.ts` | 1-52 (new) | Implements the same 10-second `.env.lock/` protocol as Rust, with cleanup on success/error and no unsafe stale-lock takeover. |
| `src/regent-cli/src/shared/infrastructure/storage/dotenvLock.test.ts` | 1-25 (new) | Verifies lock release on success and exceptions. |
| `src/regent-cli/src/shared/infrastructure/storage/dotenvFile.ts` | 1-97 (new) | Central read/update/write path with validation, duplicate normalization, owner-only temp protection, fsync, and atomic rename. |
| `src/regent-cli/src/shared/infrastructure/storage/lockdown.ts` | 1-38 | Uses the real Windows process principal with `icacls`; failures are closed, not ignored. |
| `src/regent-cli/src/shared/infrastructure/storage/lockdown.test.ts` | 1-62 (new) | Tests identity discovery, failure handling, and a real owner-only Windows ACL. |
| `src/regent-cli/src/features/setup/cli/setupCommand.ts` | 45-84 | Rejects command-line secrets and reads one key from stdin. |
| `src/regent-cli/src/features/setup/cli/setupCommand.int.test.ts` | 144-181 | Compiled-CLI tests prove stdin storage and pre-write command-line rejection. |
| `src/regent-cli/src/features/setup/domain/writeSetup.ts` | 7, 77-90 | Routes setup credentials through the shared writer. |
| `src/regent-cli/src/features/setup/domain/writeSetup.test.ts` | 2-4, 61-109 | Tests ACL failure preservation, duplicate cleanup, and newline injection rejection. |
| `src/regent-cli/src/features/gateway/cli/gatewayCommand.ts` | 6-11, 162 | Routes gateway credential writes through the shared writer. |
| `src/regent-cli/src/features/voice/cli/voiceFiles.ts` | 3-10 | Routes voice credential writes through the shared writer. |

## Verification already completed

| Area | Result |
|---|---|
| Full CLI suite outside restricted sandbox | **226 passed, 0 failed, 1,185 assertions**; includes real Chrome/Edge PDF and PNG rendering. |
| Desktop suite | **359 passed, 0 failed, 921 assertions**; typecheck and production Vite build passed. |
| Rust key-tool focused suite | **11 passed, 0 failed**. |
| Rust affected clippy | `RUSTFLAGS=-D warnings cargo clippy -p regent-tools --all-targets -j 2` passed. |
| Five-process CLI race | All five processes exited successfully and all five assignments survived. A lock-disabled mutation made the test fail. |
| Duplicate mutation proof | Reintroducing old duplicate behavior made the duplicate test fail; restored code passes. |
| Visual QA | Dedicated image/video panels, explanatory note, cross-listed providers, and responsive controls rendered correctly in a real browser fixture. |
| Version/protocol parity | Version `0.1.2`, call protocol `7`; **10 release-tool tests passed**. |
| Windows installer scripts | **30 passed, 0 failed**. |
| POSIX installer scripts | **18 passed, 0 failed** under Git Bash login environment. |
| Cargo advisory audit | Exit 0 with four allowed warnings (`paste`, `ttf-parser`, `anyhow`, `cxx`); no blocking vulnerability was reported. |

## Open blockers

The same hostile reviewer was stopped on owner instruction before producing a
Round-3 SHIP verdict. It made no edits, but reported these source-level findings.
They must be reproduced and resolved or explicitly rejected with evidence:

1. **Rust read failure may become an empty file.** `read_lines` currently returns
   an empty vector on some `.env` read errors. A later save could publish only the
   new key and discard existing credentials. Fail closed on read errors; distinguish
   a genuinely missing file from permission/I/O failure.
2. **Spaced duplicate syntax may evade Rust normalization.** A valid-looking line
   such as `KEY =value` may not be recognized as the same assignment as `KEY=value`.
   Define the accepted `.env` grammar once and mutation-test duplicate removal.
3. **Noncanonical numbered slots are inconsistent.** CLI/agent may accept `_02`
   while the deacon/listing policy accepts only `_2.._8`. Reject all noncanonical
   forms at every entry point.
4. **Gateway and voice still accept secrets as command arguments.** Although their
   writers are now secure, secret acquisition can still leak into shell history and
   process listings. Add stdin/file-descriptor flows or clearly deprecate the unsafe
   flags with tests.
5. **Gateway variable-name mismatch.** The reviewer reports that some Slack and
   WhatsApp names written by setup differ from names read by the runtime. Audit each
   gateway writer against its runtime consumer and add one contract test per platform.

No SHIP verdict exists after these findings. Do not push or release this working tree
until the same independent reviewer rechecks the fixes and returns SHIP.

## Full gates still required after blocker fixes

Run the repository's own gates in this order:

```text
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo clippy --workspace --exclude regent-voice-server --all-targets -j 2
RUSTFLAGS="-D warnings" cargo test --workspace --exclude regent-voice-server -j 2
cargo test -p regent-voice-server -j 2

cd src/regent-cli
bunx tsc --noEmit
bunx biome check src
bun run compile
bun test                 # outside the restricted sandbox for real browsers

cd src/regent-app/Desktop
bunx tsc --noEmit
bun test
bun run build

cargo audit --ignore RUSTSEC-2023-0071
cargo deny check
bun audit commands from .github/workflows/ci.yml for all four JS workspaces
python scripts/tests/verify-versions.py
python -m unittest discover -s scripts/tests -p "test_*.py"
```

Then rebuild Rust release binaries, compile the CLI, build the Tauri NSIS bundle,
stop running Regent processes, back up and reinstall both Windows copies, and verify:

- `%LOCALAPPDATA%\Programs\Regent\bin`
- `%USERPROFILE%\.regent\bin`
- `regent --version`
- `regent doctor`
- `regent update`
- media-key set/list/remove in a temporary `REGENT_HOME`
- installed Desktop Settings panels and `computer_use` behavior

## Git ownership and staging warnings

Do **not** stage, modify, revert, or commit these owner changes:

- `src/regent-app/Installer/src-tauri/src/wire/shortcuts.rs`
- `.claude/`

`docs/reference/` is removed from Git's index but preserved locally and ignored.
The three staged deletions are intentional per the owner's explicit instruction.

No task changes have been committed or pushed in this handoff. Before committing,
review the staged/unstaged split carefully and stage task files explicitly rather
than using a blanket add.

