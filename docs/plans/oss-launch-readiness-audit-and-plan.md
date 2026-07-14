# OSS Launch Readiness — Audit & Plan (v0.1.0-alpha)

Audited 2026-07-14 on branch `feat/desktop-vite`. Read-only audit except
[docs/PROJECT-OVERVIEW.md](../PROJECT-OVERVIEW.md) (deliverable, written).
Re-verified later the same day after the WIP was committed (645bf83, 7d50163) —
stale findings updated in place; the third review pass added the **onboarding
audit** (first-run driven for real, see §3).

**TL;DR: NO-GO today — but close.** The code is in better shape than the
packaging: builds clean, all tests run pass (46/46 CLI, 89/89 across four core
crates), errors surface properly, and the first-run onboarding actually works
(driven end-to-end this audit). What blocks launch is legal/licensing (no
LICENSE file, a trial-licensed npm package, unlicensed vendored code) and
~750 MB of junk in git history. **Neither the CLI nor the app has any
uninstall path.** Roughly 4–5 focused days to GO.

**Owner decisions (2026-07-14, final):** the constitution stays **always-on
by design** for the main distribution (supersedes the ADR-028 opt-in reading
— update that ADR), and the **KONTES font stays tracked in the repo**. Both
findings below are marked accordingly and are not launch work.

---

## §1 OSS hygiene — **NO-GO**

| Finding | Location | Severity | Fix |
|---|---|---|---|
| No LICENSE file anywhere, while README badge + footer claim MIT | repo root; `README.md:10`, `README.md:128` | **BLOCKER** | Add `LICENSE` with MIT text |
| KONTES font tracked (personal-use license) | `python-voice-server/ui/fonts/` | **ACCEPTED** — owner decision 2026-07-14: the font stays. Residual redistribution risk acknowledged; revisit only if the font author objects | — |
| `gsap-trial` (commercial trial license) in dependencies — unused in source; now also in the committed `package-lock.json` | `src/regent-app/Desktop/package.json:25` | **BLOCKER** | Remove the line, regenerate lockfile |
| Vendored `or-core`/`or-mcp` have a provenance README but no LICENSE | `src/crates/regent-orchustr-core/` | **BLOCKER** | Add license text. If Orchustr is the owner's own repo (the README implies a sibling checkout), MIT-cover it with one line; if third-party, copy the upstream license (mirror paddle-ocr-rs) |
| ~180 MB of ONNX model blobs tracked at HEAD (two `.fastembed_cache` copies) — ignore rule added after the fact | `src/crates/regent-embed/.fastembed_cache/`, `src/regent-cli/.fastembed_cache/` | **BLOCKER** | `git rm -r --cached` both dirs |
| History holds 5×95 MB `.bun-build` artifacts, an 86 MB model blob, two `.psb` files → public clone ~750 MB | git history | **BLOCKER** | `git filter-repo` before first push — no remote exists yet, so a rewrite costs nothing today |
| Personal machine paths and username in tracked files | `docs/development/README.md:50`, `docs/changelogs/CHANGELOG.md:4919`, `src/regent-cli/notes/2026-06-20-telegram-gateway-setup.md:5`, several `docs/proposal/*` | SHOULD-FIX | Scrub `D:\1-1@k\…` / `C:\Users\…` → generic placeholders |
| No SECURITY.md (product executes commands, holds API keys) | repo root | SHOULD-FIX | Add reporting policy + supported-versions note |
| No root `CONTRIBUTING.md` — guide at `contributions/README.md` won't be auto-surfaced by GitHub | repo root | SHOULD-FIX | One-line pointer file |
| No CODE_OF_CONDUCT, no issue templates | `.github/` | NICE-TO-HAVE | Add at leisure |

Secrets: **clean** — no key patterns in tracked files (only the redactor's own
test fixtures), no `.env`/pem/key files in the 446-commit history, `.gitignore`
covers secrets. Versioning coherent: `0.1.0` in all four manifests; date-based
changelog is fine for v0.x.

## §2 Docs — **GO** once the README header links are fixed (one blocker; the rest is cosmetic)

Docs are above the alpha bar: `docs/README.md` is a real entry point, QUICKSTART
walks install→provider→chat, command/env references exist, 36 ADRs. README
passes the 60-second test.

| Finding | Location | Severity | Fix |
|---|---|---|---|
| Header links "Regent Scout \| Regent Desktop" both point to `hermes-agent.nousresearch.com` | `README.md:7` | **BLOCKER** | Point at real pages or delete the line |
| Discord badge is a duplicate of the Ollama badge | `README.md:14` | SHOULD-FIX | Fix or drop |
| "32 ADRs" claimed, 36 exist | `README.md:37`, `README.md:107`, `docs/README.md:30` | SHOULD-FIX | Say "36" or "30+" |
| Links to `README.md#install` but heading is "Quick Install" → dead anchor | `docs/QUICKSTART.md:6`, `contributions/README.md:23` | SHOULD-FIX | `#quick-install` |
| ~~Orphan working files at docs root~~ moved 2026-07-14 into `docs/changelogs/`, `docs/fixes-notes/`, `docs/handoff's/` — the move broke every `docs/CHANGELOG.md` link; **links + folder map repaired same day** (README, docs/README, contributions, proposal) | fixed | SHOULD-FIX (remainder) | Rename `docs/handoff's/` → `docs/handoffs/` (apostrophe in a dir name is a shell/CI quoting hazard) |
| Desktop app has no build/dev guide; absent from root README | `docs/development/` | SHOULD-FIX | Add `development/desktop.md`; one README line marking it experimental |
| `Regent-Desktop-TASK.md` is an internal task file with local paths | `src/regent-app/Desktop/Regent-Desktop-TASK.md:14` | SHOULD-FIX | Delete or move to docs/plans, scrubbed |

Feature↔doc parity spot-checks passed. The "full audit trail" positioning means
internal docs (plans, audits, hermes-study) *should* ship — scrub, don't delete.

## §3 Regent CLI — **GO**

- `tsc --noEmit` clean; `bun test` **46 pass / 0 fail**; `cargo check -p
  regent-deacon` clean; `cargo test` on regent-kernel/-store/-cron/-graph
  **89 pass / 0 fail**. (Full `--workspace` suite and a true clean-checkout
  build must be proven by CI — Phase 5.)
- Command walk: `help` (clear, grouped), `status` (works, auto-finds deacon),
  bare `code` / `model set` → clean usage errors, unknown command → error +
  help + exit code 1. No panics, no swallowed errors.
- **Onboarding driven for real** (fresh `REGENT_HOME`, third pass): bare
  `regent` with no config.yaml correctly enters the setup wizard
  (`router.ts:61`); wizard completes, writes `config.yaml` (+ `.env` 0600 when
  a key is given), and `regent doctor` afterwards correctly flags the missing
  API key with exit 1. The happy path works; the problems are in the table.

| Finding | Location | Severity | Fix |
|---|---|---|---|
| Setup enables the constitution unconditionally | `setupCommand.ts:73-75` | **ACCEPTED** — owner decision 2026-07-14: always-on by design for the main distribution. Update ADR-028 to match; one disclosure line in README is honest OSS practice | — |
| Setup parses a `--constitution` flag it never reads (dead flag — the value is always-on by design) | `setupCommand.ts:29` | NICE-TO-HAVE | Delete the dead flag parsing |
| QUICKSTART §4 documents a top-level `provider:` key in config.yaml; setup actually writes `model.provider` | `docs/QUICKSTART.md:76` vs `setupCommand.ts:209` | SHOULD-FIX | Verify what the deacon reads; fix whichever is wrong |
| Non-TTY first run (piped stdin) silently accepts all defaults and exits 0 | `setupCommand.ts:154` (`prompt()` → null → default) | NICE-TO-HAVE | Acceptable — the no-key warning still prints; note it in docs |
| `config set` silently accepts unknown keys (`foo.bar baz` wrote to config.yaml with a success message) | `src/regent-cli/src/features/config/` | SHOULD-FIX | Warn or reject keys not in the schema |
| Compiled binary is 99 MB (Bun embeds its runtime) | `src/regent-cli/dist/` | NICE-TO-HAVE | Acceptable; documented in PROJECT-OVERVIEW |

## §4 Regent Desktop — **NO-GO** (as sitting on disk at audit time)

- `npm run build` (tsc + Vite): **clean, 1.8 s**. No trial-plugin imports in
  source. Launch/chat/settings/voice flows not driven end-to-end in this audit
  (need Tauri shell + deacon; the 07-13 session notes say they work but that
  work was uncommitted).

| Finding | Location | Severity | Fix |
|---|---|---|---|
| `gsap-trial` dependency (see §1) | `src/regent-app/Desktop/package.json:25` | **BLOCKER** | Remove |
| ~~60 uncommitted files~~ **committed 2026-07-14** (645bf83 docs reorg, 7d50163 icons/deps/lockfile); **main is still 257 commits behind** | working tree → clean | **BLOCKER** (merge part) | Merge `feat/desktop-vite` → main; launch from main |
| ~~Desktop `package-lock.json` untracked~~ **committed 2026-07-14** — note it locks `gsap-trial` too, so Phase 1's regenerate matters | `src/regent-app/Desktop/` | done | — |
| ButlerView chunk 2.96 MB (823 KB gz) | build output | NICE-TO-HAVE | Code-split later |
| No Tauri bundle/installer produced or verified | `src-tauri/` | SHOULD-FIX | Ship as experimental/source-only for alpha (recommended) |

## §5 Installer / distribution — **NO-GO** (nothing to install from yet)

- `scripts/install.sh` / `install.ps1` are well-written: release-first with
  source fallback (sh) / clear failure message (ps1), prereqs checked with
  URLs, PATH handled, `REGENT_REPO`/`BIN_DIR` overridable.
  `.github/workflows/release.yml` builds exactly the asset names the installers
  download, 4 OS/arch targets, on `v*` tags.
- **But:** no git remote, no GitHub repo, no tags → both one-liners 404 today.
  **BLOCKER** — create repo, push, tag, verify a real release run.
- Download size: regent-cli 99 MB + regent-deacon 41 MB → ~140 MB unpacked,
  **~50–60 MB compressed** per platform (estimate). If the target is under
  50 MB, the only lever is dropping Bun single-binary packaging — not worth it
  for alpha.
- Prereqs: LLVM/libclang for voice documented
  (`docs/development/voice-and-api-calls.md`); voice correctly excluded from
  default build/release. Windows install needs no dev tools. ✓
- Voice server and desktop app are not in the release asset — fine for alpha,
  but say so in README (one line).

## §6 Deliverable — **done**

[docs/PROJECT-OVERVIEW.md](../PROJECT-OVERVIEW.md): one-paragraph architecture
with diagram, repo map, design decisions in plain language with ADR pointers,
features keyed to commands, honest known-edges list. Link it from
`docs/README.md` during the docs pass.

## §7 Uninstall — **NO-GO. Neither the CLI nor the app has one.**

The only "uninstall" mentions in the repo are a gap-analysis note and a parity
plan explicitly deferring it (`docs/proposal/cli-command-parity-plan.md:202`).

| Installed artifact | Created by | Removed by |
|---|---|---|
| `~/.regent/bin/` (regent-cli, regent-deacon, `regent.cmd` shim) | both installers | nothing |
| **User PATH registry edit** (Windows) | `scripts/install.ps1:34` | nothing |
| `~/.local/bin/regent` symlink (unix) | `scripts/install.sh:48` | nothing |
| Running deacon / gateway (`gateway.pid`) / voice server | first run | nothing — deleting files leaves them running |
| `~/.regent` user data (config.yaml, .env keys, SQLite sessions/memory, logs, skills) | first run | nothing; keep-vs-delete choice documented nowhere |
| `~/.regent/src` (source-fallback clone, can be GBs with `target/`) | `scripts/install.sh:39` | nothing |
| Desktop: no installer exists yet, so no OS uninstaller entry | — | — |

Partial-install and app-running cases: unhandled. **BLOCKER** for public alpha —
"how do I get rid of it" is a guaranteed first-week issue.

---

# Launch plan (v0.1.0-alpha)

Drafted, reviewed, refined, re-reviewed fresh, then gap-checked — the passes and
what they changed are summarized after the plan.

### Phase 0 — Reconcile the tree (½ day)
1. ~~Commit the 60-file WIP~~ **Done 2026-07-14** (645bf83, 7d50163 —
   including Desktop `package-lock.json`).
2. ~~Constitution opt-in~~ **Dropped — owner decision 2026-07-14: always-on
   by design.** Update ADR-028 wording to match during the docs pass.
3. Merge `feat/desktop-vite` → `main`. Main becomes the launch branch; delete
   or freeze stale branches.
4. Full local gate on main: `cargo test --workspace` (voice excluded),
   `bun test`+`tsc`+`biome` in regent-cli, `npm run build` in Desktop. Record
   in CHANGELOG.
   **Exit:** main green, working tree clean.

### Phase 1 — Legal & licensing (1 day) ← gates everything
1. Add MIT `LICENSE` (root).
2. Remove `gsap-trial` from Desktop package.json + lockfile.
3. ~~Untrack KONTES font~~ **Dropped — owner decision 2026-07-14: the font
   stays tracked.** Residual personal-use-license risk accepted.
4. Add license text to `src/crates/regent-orchustr-core/` (provenance README
   already exists): if Orchustr is first-party, state MIT in one line; if
   third-party, copy the upstream license (mirror paddle-ocr-rs).
5. Run `cargo deny check licenses` (deny.toml exists — wire into CI too) and a
   JS license scan (`npx license-checker` on both package.json trees). Fix
   anything copyleft-incompatible.
6. One line in README/overview noting Kokoro/whisper model weights download at
   runtime under their own licenses.
   **Exit:** every byte in the repo is redistributable under a stated license.

### Phase 2 — History & size (½ day, must precede first push)
1. `git rm -r --cached` both `.fastembed_cache` trees; commit. (The `.psb` is
   already deleted at HEAD as of 7d50163 — it only needs the history purge in
   step 3.)
2. Scrub personal paths/username from tracked docs (§1/§2 lists — mechanical
   sed pass, review diff by hand).
3. `git filter-repo` to purge from history: `*.bun-build`,
   `.fastembed_cache/`, `*.psb`, deleted logo JPG/PNGs. Verify with
   `git count-objects -vH` → repo well under ~50 MB.
4. Re-run secret scan (gitleaks) on the rewritten history as the final
   pre-push gate.
   **Exit:** fresh clone is small and clean; nothing personal or heavyweight in
   any commit.

### Phase 3 — Docs & repo furniture (½–1 day)
1. Fix README: header links (drop or real URLs), discord badge, ADR count; add
   an "Uninstall" section and one "Desktop app (experimental, build from
   source)" line.
2. Fix `#install` anchors (changelog links already repaired 2026-07-14);
   rename `docs/handoff's/` → `docs/handoffs/`; delete or scrub
   `Regent-Desktop-TASK.md` and `regent-cli/notes/`.
3. Add: SECURITY.md (private reporting + alpha caveat), root CONTRIBUTING.md
   pointer, link PROJECT-OVERVIEW.md from docs/README.md,
   `docs/development/desktop.md` (10 lines: install, tauri dev, needs local
   deacon).
4. Soften or verify marketing claims ("17+ platforms", "recall@5 ≥ 0.75 in CI"
   — the latter must actually run in CI, see Phase 5).
   **Exit:** a stranger goes README → install → first command → uninstall with
   no dead link.

### Phase 4 — Uninstall (½ day)
1. `scripts/uninstall.sh` + `scripts/uninstall.ps1`: stop deacon/gateway/voice
   (pidfile first, process-name fallback, works while the app is running),
   remove `~/.regent/bin`, shim, symlink, Windows PATH entry; idempotent so
   partial installs uninstall cleanly; keep `~/.regent` data by default, print
   where it lives, `--purge` deletes it (including `~/.regent/src`).
2. Document in README + QUICKSTART. Test: install → run once → uninstall →
   verify PATH/registry/processes clean; uninstall again (idempotence).
   **Exit:** both one-liners have a mirror-image goodbye path.

### Phase 5 — Publish & prove (1 day, mostly waiting on CI)
1. Create the GitHub repo (confirm `Regent33/Regent` is final — installers
   hardcode it), push rewritten main.
2. CI must pass from a **clean checkout** on Linux/macOS/Windows — the real
   "builds from clean checkout" proof. Add cargo-deny license check and
   gitleaks to CI.
3. Tag `v0.1.0-alpha` (matches `v*` in release.yml) → verify all 4 release
   assets appear; record actual compressed sizes in release notes.
4. On throwaway VMs (no dev tools): run both install one-liners,
   `regent doctor`, chat with Ollama and one hosted provider, then the
   uninstall script. Windows: note the SmartScreen warning for unsigned
   binaries in README (signing is post-alpha).
   **Exit:** a stranger's machine goes zero → chatting → cleanly removed.

### Phase 6 — Launch dressing (½ day)
1. Release notes: what works, what's alpha (desktop = experimental/source-only,
   voice = optional/LLVM), no-telemetry statement, how to report issues.
2. Issue templates (bug/platform); pin a "known limitations" issue mirroring
   PROJECT-OVERVIEW's edges list.
3. Ship.

**Total: ~4–5 focused days.** Critical path: Phase 1 → 2 → 5 — everything legal
and heavy must be fixed *before* the first push, because after that history is
public forever.

---

## What the review passes changed

**Pass 1 (self-review):** added cargo-deny/license-checker (a LICENSE file
alone doesn't prove the *dependency tree* is clean); moved gitleaks to *after*
the history rewrite; demoted desktop from "ship an installer" to "experimental,
source-only" — it needs a locally built deacon, so a packaged app would be
broken out of the box; added the repo-name confirmation step since both
installers hardcode `Regent33/Regent`.

**Pass 2 (fresh eyes):** caught that vendored `or-core`/`or-mcp` carry no
license at all — upgraded to a Phase-1 blocker; added SmartScreen/unsigned
expectations for Windows; added the "claims audit" (ADR count was already
wrong — the 17-platforms and CI-eval-gate claims deserve the same check);
added uninstall *idempotence* and app-running tests; noted `~/.regent/src`
must be handled by uninstall's `--purge`.

**Pass 3 (fresh eyes, later 2026-07-14, post-commit):** actually **drove the
onboarding** (fresh `REGENT_HOME` → bare `regent` → wizard → `doctor`), which
the first two passes never did — found the forced-on constitution
contradicting ADR-028 (new blocker, Phase 0), the dead `--constitution` flag,
and the QUICKSTART `provider:`-vs-`model.provider` schema mismatch. Fixed the
garbled KONTES row (it said "can ship"/"track + git it" — inverted; it's a
personal-use font, severity raised to BLOCKER to match Phase 1). Marked
stale-by-commit findings done (WIP committed, lockfile tracked, `.psb` deleted
at HEAD, orphan docs moved) and caught that the docs move **broke the
changelog links** (fixed same day). Aligned the TL;DR day estimate with the
plan total. Noted the or-core README *does* carry provenance — the gap is the
license text only, and the fix depends on whether Orchustr is first-party.

## Remaining gaps — unverified by this audit / open questions

1. **Full workspace tests never ran here** — 4 crates + CLI proven; Phase 5's
   clean-checkout CI on three OSes is the only real proof (no CI history —
   no remote).
2. **Desktop core flows unverified end-to-end** — launch/chat/settings/Butler
   call not driven; click through them post-merge.
3. **Gateway platforms untested** — 17 adapters in code, none exercised here.
   Label honestly which have actually been used (Telegram has; most others
   likely haven't).
4. **Name collision** — "Regent" unchecked on GitHub/crates.io/npm/trademark.
   Cheap to check, expensive to discover late.
5. **`regent migrate hermes` provenance** — ensure nothing vendored *from*
   Hermes (prompts, skill files) crossed from "studied" to "copied" without
   license compatibility; grep for verbatim blocks before launch.
6. **Python voice server** — shipped, unpinned, duplicates the Rust server.
   Consider dropping it from the public tree (less to license-audit/support).
7. **Release-size target undecided** — actuals ~50–60 MB compressed per
   platform; if unacceptable, the decision changes Phase 5, so decide before
   tagging.
8. **Post-alpha debts to track now:** binary signing (Windows/macOS
   notarization), auto-update, `regent uninstall` as a first-class command,
   desktop packaging with a bundled deacon, a `config set` schema guard.

---

*Audit notes: one side effect during testing — `regent config set foo.bar baz`
wrote to the live `~/.regent/config.yaml` (proving the unknown-key finding);
reverted immediately. Test evidence: CLI 46/46 pass; regent-kernel/-store/
-cron/-graph 89/89 pass; `cargo check -p regent-deacon` clean; Desktop Vite
build clean in 1.8 s. Pass 3 onboarding evidence: fresh-`REGENT_HOME`
first-run setup completed exit 0 (writes `config.yaml` with
`constitution.enabled: true` unconditionally — the §3 blocker); `regent
doctor` on that home correctly failed exit 1 on the missing API key; scratch
home used, live `~/.regent` untouched.*
