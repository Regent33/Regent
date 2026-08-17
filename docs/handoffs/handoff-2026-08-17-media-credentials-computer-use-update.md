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

## Open blockers — ALL FIVE NOW CLOSED (verified 2026-08-17, Claude)

Every one was reproduced against source before being fixed, and each fix has a
test that fails when the defect is put back. Two of them were worse than the
reviewer described, and one was real in a different place than claimed.

| # | Verdict | Evidence |
|---|---|---|
| 1 | REAL — data loss | `read_lines` turned ANY read error into an empty file, and the write path is read-modify-publish: one new key then atomically replaced every credential, reporting success. NotFound alone now means empty. Reintroducing the swallow fails the new test with `an unreadable .env must not read as empty: []`. |
| 2 | REAL — worse than reported | Not cosmetic: the LOADER trims the name after splitting on `=`, so `KEY =value` is live, while `is_key_line` required `=` to touch the key. `remove_env_var` therefore skipped that line and the deleted credential kept authenticating on the next boot. Writer and loader now share one grammar. |
| 3 | REAL | `"02".parse::<u8>()` is `Ok(2)` and `Number("02")` agrees, so `KEY_02` was settable on both surfaces but never listed and never resolved — a credential saved into a hole. One canonical spelling, rejected at the boundary. |
| 4 | REAL | Gateway and voice now read secrets from a pipe via one shared `secretStdin.ts`. The argv forms are REMOVED rather than warned about: by the time a warning could print, the shell has logged the token and `ps` has read it. |
| 5 | REAL, but NOT where reported | The claim was Slack/WhatsApp name mismatches; the diff shows none. What the audit actually found is three names the runtime reads that no writer would accept: `TWILIO_VOICE_GREETING` (registry.rs:71 gates the Twilio VOICE adapter on it, so phone calls could not be enabled through any supported path) and `AZURE_DEVOPS_BASIC_USER`/`_PASS` (registry_ext.rs:84-85). Not mismatched — unreachable. `platform_env.rs` is now the contract and a table-driven test derives both sides from source. |

### Also found while verifying, not in the reviewer's list

- **CLI/app catalog drift.** `REGENT_BROWSER_MCP_URL` existed only in the
  TypeScript mirror, so `regent keys set` accepted what the deacon rejected and
  the app never showed — while `status_ops.rs` had just been changed to tell
  users to run that exact command. Five more keys were filed in different
  sections per surface. `scripts/tests/verify-key-catalog.py` now checks names,
  groups, AND that every group the deacon can emit is one the Desktop page
  actually renders — the last of those is the mechanism that made the media
  rows vanish, since `visibleApiKeys` filters to `API_KEY_GROUPS`.
- **A throwaway QA probe** was left in `regent-tools/tests/`; it `.expect()`s an
  env var, so it would have panicked in `cargo test --workspace` and broken CI.
  Deleted.
- **Stale docs.** `PROJECT-OVERVIEW.md` still advertised `regent gateway setup
  <token>`, the form now refused; QUICKSTART documented two of the four stdin
  flags. Both corrected.

### One observation left open, deliberately not "fixed"

`regent-store`'s `read_does_not_block_behind_held_write` failed ONCE at 584ms
against its 300ms budget, during a workspace run that shared the disk with a
Vite production build and the bun suite. It did not reproduce in 17 further
runs: 6 idle, 5 under 16-way CPU load, 6 of the full 62-test parallel suite —
and the clean workspace run afterwards was 1323 passed / 0 failed. CI has run
it green three times today.

It is recorded rather than silenced. Widening the budget would make it green
without making it true, which is the same move that let round 8's defect
through. The honest state: a 300ms assertion against a 500ms held write is
load-sensitive by construction, and someone should make it deterministic
(measure lock acquisition rather than wall-clock, or drive the writer with a
barrier) instead of trusting a stopwatch. Do NOT conclude "flake" from local
greens — that reasoning was wrong about the screening test this morning, and
CI failed it twice afterwards.

### The reviewer's original text, for the record

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

*(Above: the reviewer's five findings as originally written. All five are closed
per the table at the top of this section.)*

## Gate results after the fixes (2026-08-17)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --exclude regent-voice-server --all-targets` (`-D warnings`) | exit 0 |
| `cargo test --workspace --exclude regent-voice-server` | **1323 passed, 0 failed** |
| `cargo test -p regent-gateway` | 108 passed (incl. the new credential-name contract) |
| CLI: `tsc --noEmit` / `biome check` / `bun test` | clean / clean / **235 passed, 0 failed** |
| Desktop: `tsc --noEmit` / `bun test` / `bun run build` | clean / **359 passed** / built |
| Installer crate: `cargo test` (`-D warnings`) | 11 passed |
| `python scripts/tests/verify-versions.py` | 0.1.2, call protocol 7, aligned |
| `python scripts/tests/verify-key-catalog.py` | **116 keys agree; image=11, video=10** |
| `python -m unittest discover -s scripts/tests` | 10 passed |

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

