# 2026-07-23 installer, chat, document, and security fixes

Status: implemented; automated integrated verification green; final Fable/manual smoke pending.

## Context

On 2026-07-23, logs and the state ledger show two concrete regressions: a voice comparison was misrouted into `create_document`, silently discarded unsupported `items/points` fields, produced title-only PPTX slides, and delayed speech by ~43 seconds; a later document turn exceeded the OpenAI-compatible client's 60-second read timeout after three large web-search payloads and all 36 deferred tool schemas expanded the repeated context, yielding the misleading `error decoding response body` message. The same day's music request used 244,898 cumulative input tokens, including eight `session_search` calls. Installer review also confirmed missing cmd.exe docs, no automatic CLI setup launch, no Windows Start Menu shortcut, one-liner uninstall drift, weak small-header typography, incomplete release guidance, and unverified downloads.

The goal is to fix the observed failures with the existing dynamic document renderer, reduce avoidable context growth, complete installer/uninstaller behavior, and publish an evidence-based current security/completeness assessment without overclaiming. The paused document-pipeline redesign is not resumed.

## Reviewed implementation plan

### 1. Voice comparison and document-input safety

- `src/crates/regent-agent/src/domain/prompts/system.rs` — explicitly distinguish an explanation/comparison from an explicitly requested file: voice explain/compare turns must use the existing inline visual-explainer block and must not call `create_document`/`background_task`; bump the prompt schema marker so resumed sessions receive the fix.
- `src/crates/regent-agent/src/domain/prompts/mod.rs` and existing resume/prompt tests — pin the new marker and prohibition without weakening existing frozen/custom-prompt behavior.
- `src/crates/regent-tools/src/infra/create_document/model.rs` — reject unknown nested section/slide/sheet/image fields with serde rather than silently dropping content.
- `src/crates/regent-tools/src/infra/create_document/model.rs` or a small sibling validation module only if needed to stay under 200 lines — reject unsupported PPTX layout names before synthesis instead of silently falling back.
- `src/crates/regent-tools/src/infra/create_document/tests/round_trip.rs` / existing create-document test modules — reproduce the exact `layout:"compare"` + `items/points` incident and assert a descriptive tool error; retain existing tests proving generated per-document themes and varied layouts.

No new PPTX compare layout, renderer rewrite, or native-writer deletion: the existing inline compare renderer is the right path for an explanation, and the current file renderer already generates content-derived palettes across PPTX/PDF/DOCX/XLSX.

### 2. Provider cutoff and token-efficiency fixes

- `src/crates/regent-providers/src/infra/openai_compat.rs` — remove the redundant 60-second reqwest read timeout while preserving the 10-second connect timeout and existing 120-second total request timeout. This lets slow reasoning-model prefill reach the first SSE byte without allowing an unbounded call.
- `src/crates/regent-providers/src/infra/openai_stream.rs` and the non-streaming adapter — classify reqwest timeout failures as timeouts rather than opaque body-decode/network strings.
- `src/crates/regent-deacon/src/application/dispatcher/prompt_ops/turn_errors.rs` — show a short actionable timeout message in chat/voice and add regression coverage.
- `src/crates/regent-tools/src/infra/web_search.rs` + tests — cap each returned snippet at a UTF-8-safe fixed length while preserving the 12-source breadth policy, rank, URLs, and valid JSON.
- `src/crates/regent-tools/src/infra/memory_tools/session_tools.rs` + tests — clamp requested result counts and bound snippets so one recall cannot inject 50 large hits; strengthen tool guidance toward one broad query before refinements.

The existing `reveal_all_deferred` recovery remains in this pass because it fixes a documented weak-model tool-starvation failure. The audit will record its 36-schema cost and recommend a separately evaluated targeted-recovery change rather than trading one reproduced bug for another.

### 3. Installer, uninstaller, Windows Search, and typography

- `scripts/install.ps1` / `scripts/install.sh` — after a successful one-line install, launch `regent setup` automatically when an interactive terminal is available; skip for GUI/offline embedding and an explicit no-launch environment flag. POSIX reattaches stdin from `/dev/tty` so `curl | sh` cannot consume setup input from the script pipe.
- `scripts/uninstall.ps1` — stop the desktop `Regent` process and remove the stale user `REGENT_DEACON_PATH` pin, while preserving data unless purge is explicit.
- `src/regent-app/Installer/src-tauri/src/wire.rs` plus focused `wire/shortcuts.rs` and `wire/uninstall.rs` modules — always create a Windows Start Menu `Regent.lnk` independently of the optional Desktop shortcut, remove it during uninstall, reuse the existing escaped WScript.Shell path boundary, and move rather than duplicate code so every touched source file is under 200 lines.
- `src/regent-app/Installer/app/ui/Logo.tsx`, and `Finish.tsx` / `Removed.tsx` only if visual review shows the same issue — keep Chorus for the large brand wordmark; use readable Archivo semibold treatment for small installer/uninstaller page headers.
- `README.md` and `docs/QUICKSTART.md` — add a real cmd.exe one-liner, copy/paste uninstall commands, direct GitHub Releases links for GUI installers, exact one-liner vs GUI default install locations, automatic setup behavior, preserved-data behavior, and honest platform/signing limits.

The two supported install locations remain documented rather than silently unified: one-line CLI installs under the Regent home/bin path; GUI installs under the platform app location.

### 4. Supply-chain hardening (approved)

- `.github/workflows/release.yml` — generate a per-asset `<asset>.sha256` beside every CLI/deacon archive in the same matrix job and upload both atomically, avoiding a cross-run aggregate-manifest race.
- `.github/workflows/installer.yml` — publish matching checksums beside the Windows NSIS and Linux AppImage GUI installers. Do not add a macOS DMG job until Apple Developer ID signing is available.
- `scripts/install.ps1` / `scripts/install.sh` — download the matching checksum first, verify the Regent release archive before extraction, and fail closed on missing/mismatched verification. The GUI offline payload remains trusted through its attested installer build and explicitly skips the network verification path.
- Pin each optional third-party ffmpeg download to a reviewed version and hardcoded SHA-256. If a platform has no stable verifiable artifact, stop auto-downloading there and print the existing package-manager hint; ffmpeg remains optional and non-fatal to Regent itself.
- Add script/CI checks for success, mismatch, missing checksum, quoted/UTF-8 paths, reinstall, and offline GUI payload behavior.

### 5. Voice execution hardening (approved)

- `src/crates/regent-tools/src/domain/contracts.rs`, its focused tests, and `src/crates/regent-deacon/src/application/session_manager/session_ctx.rs` — make the default live-voice approval handler deny every mutating approval request, not only terminal. Read-only screen/vision remains available because it does not cross the mutation gate. Preserve `REGENT_VOICE_FULL_CONTROL=1` as the explicit route to `AllowAll` for users who knowingly want hands-free mutation.
- `src/crates/regent-voice-server/src/infra/spawn.rs`, `docs/reference/env-vars.md`, and `docs/QUICKSTART.md` — align comments and user guidance with the actual opt-in contract. Keep computer-use registration available so full control can be enabled without a rebuild/restart mismatch.
- Tests must prove default voice denies terminal, computer-use mutation, file mutation, and browser/app mutation requests; full-control voice still selects blanket approval; non-voice auto mode remains unchanged.

### 6. Security and completeness audit/report

- `docs/audits/2026-07-23-security-completeness-audit.md` — update the 2026-07-02 findings against current code, separated into confirmed protections, confirmed residual gaps, resolved historical findings, and unverified surfaces.
- Cover `regent-store`, gateway/webhooks, deacon HTTP/stdio boundaries, tools/path jail/sandbox/approval, skills/agents/prompt injection, secrets, installer provenance, and GUI/CLI uninstall completeness.
- Record current evidence including the store's separate WAL read connection, default-deny HTTP token, external-session jail/auth/rate limits, owner-only secret storage, SSRF IP pinning, the newly opt-in voice mutation posture, local-session sandbox opt-in, ignored RSA advisory, and supply-chain status.
- Include the 2026-07-23 token ledger: 123,041 input tokens/6 calls for the failed deck session and 244,898/14 calls for the music request; explain repeated full-context sends, large raw tool outputs, repeated search, cache resets, and the one-time deferred-schema expansion.
- Do not claim any component is “secure” absolutely; give a prioritized remediation table and explicit unverified list.

### 7. Changelog and execution discipline

- Update `docs/changelogs/CHANGELOG.md` after each verified atomic phase, matching its evidence-led style. No ADR is needed unless an approved security/release policy creates a new lasting contract.
- Re-read every target immediately before editing because parallel sessions use this repository.
- Never stage or modify unrelated untracked `.vscode/tasks.json`, `scripts/tools/claude-codex.ps1`, or `scripts/tools/tempCodeRunnerFile.ps1`; the local proxy token must not enter git and should be rotated separately by the user.
- New files stay below 200 lines; touched oversized files shrink where the change provides a natural split. Existing tests are not deleted or weakened.
- After plan approval, delegate independent implementation phases to Opus 4.8 agents with maximum-rigor prompts; parent then reviews every diff/test first, fixes defects directly, and runs the final integrated verification.

## Verification

- Core Rust: `cargo fmt --all -- --check`; `cargo clippy --workspace --exclude regent-voice-server --all-targets -- -D warnings`; package tests for `regent-agent`, `regent-tools`, `regent-providers`, `regent-deacon`; final `cargo test --workspace --exclude regent-voice-server`.
- Installer Rust: run `cargo test` with `RUSTFLAGS=-D warnings` in `src/regent-app/Installer/src-tauri` (outside the workspace).
- Installer UI: from `src/regent-app/Installer`, run `bun run typecheck` and `bun run build`; visually run the installer/uninstaller screens (this package currently has no separate lint/test scripts).
- Scripts: PowerShell parser/scratch-home install and uninstall; `sh -n`; interactive and non-interactive launch guards; checksum mismatch tests if approved.
- Incident A end-to-end: ask Butler “Ferrari vs Lamborghini — which is more practical?”; assert the persisted assistant response begins with inline compare JSON, no `create_document` call exists, diagram reaches the client before speech, and no PPTX is created. Direct malformed `create_document` input must fail before writing a file.
- Incident B end-to-end: simulate delayed first SSE byte beyond 60 seconds but below 120 and verify success; force a total timeout and verify the actionable message; verify bounded web/session-search payloads and valid citations/results.
- Windows installer smoke: install, confirm Start Menu search finds Regent, run setup automatically, launch the app, uninstall, and verify shortcuts, binaries, PATH entry, and `REGENT_DEACON_PATH` are removed while user data remains.
- File-size check on every touched/new source file; inspect `git diff --check` and final `git status` for unrelated files.

## Decisions resolved

- Release and GUI installer assets publish per-file SHA-256 sidecars; one-line installers verify before extraction and refuse an unverified archive. Optional Windows ffmpeg is version/hash pinned; macOS/Linux use package-manager guidance.
- Voice mutation is opt-in. The default voice approver denies every gated mutation; `REGENT_VOICE_FULL_CONTROL=1` restores blanket control.
- macOS GUI waits for Apple Developer ID signing. The verified one-line installer remains the supported macOS path.
- The existing dynamic document renderer remains. This fix rejects malformed fields and routes voice explanations to the inline diagram instead of inventing a second PPTX comparison system.

## Implemented result

- Prompt schema v4 rebases resumed Regent sessions and keeps comparison explanations inline.
- `create_document` rejects unknown nested fields and unsupported PPTX layouts before writing.
- OpenAI-compatible calls keep the 120-second total timeout but no longer die at a separate 60-second read timeout; timeout text is actionable.
- Web/session search outputs are bounded to reduce repeated-context cost.
- Windows Start Menu registration, uninstall parity, readable Setup headers, cmd.exe docs, automatic CLI setup, and release checksums are implemented.
- SYSTEM_PROMPT and CAPABILITIES remain Tier 0; the always-on constitution remains first in the persona segment and is trimmed last, never deferred with tools.
- Parent review preserved the public voice approver contract and split every touched source file to at most 200 lines.
- Hermetic production-path installer checks cover valid/mismatched/missing checksums, offline/reinstall/custom-home/Unicode paths, stale deacon pins, and zip traversal without touching real user state.
- Document rendering tries compatible Chromium browsers per platform; an installed browser that exits cleanly without output no longer blocks another candidate.