# Building Regent Setup

Regent Setup is one graphical executable that installs all of Regent: the agent
core (`regent-deacon`), the `regent` CLI, and the desktop app. It carries them
inside itself, so the machine it runs on needs no network, no Rust, and no Bun.

## What it is

A Tauri 2 app — React frontend in `app/`, Rust backend in `src-tauri/`. It uses
the Regent desktop app's design tokens verbatim, forced to the light theme.

At install time it does three things, streamed to a live log:

| Stage | What happens |
| --- | --- |
| `core` | Runs the bundled `install.ps1` / `install.sh` with `REGENT_LOCAL_ARCHIVE` pointing at the bundled archive. The script extracts `regent-deacon` + `regent-cli` into `<install_dir>/bin`, writes the shim, and adds it to PATH. |
| `app` | Copies the desktop app executable into `<install_dir>/app`. |
| `wire` | Writes the desktop shortcut and the Apps & features uninstall entry. |

The core stage reuses the *same* install script as the `curl \| sh` one-liner —
there is one implementation of "extract and put on PATH", not two.

## Build

One step, preferred — stages the payload, builds, and signs when a certificate
is configured (see Code signing below):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-setup.ps1   # -SkipPayload to iterate
```

```sh
sh scripts/build-setup.sh   # macOS / Linux; SKIP_PAYLOAD=1 to iterate
```

Or the two underlying steps by hand, in order. The payload must exist before
the app is built: the `payload/**/*` resource glob in `tauri.conf.json` fails
the build otherwise.

```sh
# 1. Stage what Setup carries (release builds of deacon, CLI, and the app)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-payload.ps1   # Windows
sh scripts/build-payload.sh                                                     # macOS / Linux

# 2. Build Setup itself
bun install
bun run tauri build
```

`build-payload` takes 10–20 minutes cold — it release-builds the deacon, compiles
the CLI to a single binary, and builds the desktop app. Iterate with `-SkipCore`
/ `-SkipApp` (PowerShell) or `SKIP_CORE=1` / `SKIP_APP=1` (sh).

Output lands in `src-tauri/target/release/bundle/`:

| Platform | Artifact |
| --- | --- |
| Windows | `nsis/Regent Setup_<version>_x64-setup.exe` |
| macOS | `dmg/Regent Setup_<version>_<arch>.dmg` |
| Linux | `appimage/Regent Setup_<version>_amd64.AppImage` |

Each is built on its own OS. There is no cross-compilation path — the payload
contains native binaries for the host. CI (`.github/workflows/installer.yml`)
builds the Windows exe and the Linux AppImage on version tags; macOS waits on
a runner + Developer ID.

On Linux the AppImage needs FUSE to run (`libfuse2` on Debian/Ubuntu); without
it, `./"Regent Setup"*.AppImage --appimage-extract-and-run` works everywhere.

### Developing

```sh
bun run dev          # browser only, http://localhost:3100 — walks a simulated install
bun run tauri dev    # the real window and the real Rust backend
```

`bun run dev` has no Tauri backend, so `runInstall` falls back to a simulation
and every screen is still reachable. `bun run tauri dev` runs the real install
code; with no payload staged it fails the `core` stage with
"no bundled payload — run scripts/build-payload.ps1 first", which is the failure
screen doing its job. Stage a payload to exercise the real path — a dev run
reads `src-tauri/payload/` straight from the source tree.

## What gets installed where

Per-user by default, but Setup asks for administrator the moment it starts, so
the location is genuinely the user's choice — `D:\Program Files\Regent` works as
readily as the default. Refusing the prompt is not fatal: the default location
never needed administrator, and an installer that will not run without admin
rights is worse than one that asks and carries on.

The ask is a self-relaunch (`elevate.rs`), not a `requireAdministrator`
manifest. A manifest is simpler and breaks the hand-off: NSIS starts us with
`nsis_tauri_utils::RunAsUser`, a CreateProcess-family call, which fails with
ERROR_ELEVATION_REQUIRED instead of prompting — the "Run Regent Setup" checkbox
would silently do nothing, and `tauri dev` would fail the same way. See
`docs/adr/ADR-036`.

**What elevation does not change: the install stays per-user.** PATH, the deacon
pin, the Apps & features entry and the shortcut are all written to `HKCU` and
the user's own Desktop. Under Admin Approval Mode that is the same account, so
this is invisible — but satisfy the prompt with a *different* administrator's
credentials and all of it lands on **that** account instead.

```
%LOCALAPPDATA%\Programs\Regent\      (default; the user can change it)
  bin\    regent-deacon.exe, regent-cli.exe, regent.cmd
  app\    Regent.exe
  uninstall.ps1
```

The desktop app finds the deacon through a persisted user `REGENT_DEACON_PATH`,
set during the `wire` stage — deliberately not via PATH. Its `find_deacon()`
does fall back to PATH, but PATH is optional here (the checkbox), and a PATH
written during install is invisible to every process that already exists,
including the app launched from the finish screen. On Linux the `.desktop` entry
carries the variable in its `Exec` line for the same reason.

macOS/Linux default to `~/.local/share/Regent`, with the CLI linked into
`~/.local/bin/regent` by `install.sh`.

The user's data — `~/.regent`, holding config, API keys, and memory — is
separate, and the uninstaller deliberately leaves it alone. Uninstalling an app
is not consent to delete someone's data.

## Uninstall

**Windows gets a GUI uninstaller: the same binary, in a second mode.**

The `wire` stage copies the running executable to `<install_dir>\uninstall.exe`,
and `mode()` routes on the name it was launched under — so Apps & features
(which passes no arguments) and a double-click in Explorer both land on the
uninstall flow. It reuses the installer's design, screens, staged progress, and
live log; only the confirm and "removed" screens are its own.

The copy costs ~10MB, not 70MB: the payload is a sibling resource, not part of
the executable, so it comes along with none of it. Uninstall mode never reads
the payload.

Two tests guard the routing (`cargo test`): getting it backwards means Apps &
features silently opens the *installer*, and `UNINSTALLER_NAME` drifting from
what `mode_for` matches would do the same.

Registered at `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Regent`,
so Regent appears in Apps & features like any other program.

**macOS/Linux get the same GUI uninstall by re-running Setup.** There is no
Add/Remove-Programs to launch a copied uninstaller from, and copying ourselves
is not portable anyway — an AppImage's `current_exe()` points inside its own
read-only mount, so the copy would be a binary that cannot run standalone (and
copying the whole AppImage would park the full ~85MB, payload included). So
Setup detects an existing install at startup — the `.desktop` entry it wrote
names the deacon, custom locations included, and the default directory is the
fallback; either candidate only counts if `bin/regent-deacon` is actually in
it — and the welcome screen offers "remove it instead", which flips into the
identical uninstall flow. The Install button reads "Reinstall" and the
location is prefilled with the *existing* directory, so reinstalling lands on
top of the old install rather than starting a second one.

An `uninstall.sh` in the install directory covers whoever deleted the
AppImage: it removes the same things (CLI link, menu entry, install dir) and
leaves the same thing alone.

**`~/.regent` is never touched, on any platform.** It holds config, API keys,
and memory; removing the program is not consent to delete someone's data.

## Code signing

Unsigned builds work but warn: Windows shows a SmartScreen "unknown publisher"
prompt, and macOS refuses to open the app at all.

### Windows (Authenticode)

**Signing is automatic when configured.** `scripts/build-setup.ps1` is the one
build entry point — it stages the payload, builds Setup, and signs it when
either of these is set (warning loudly when neither is):

```powershell
# Classic certificate installed in the store (OV/EV): its SHA-1 thumbprint
$env:REGENT_SIGN_THUMBPRINT = "<THUMBPRINT>"

# Or a sign command with a %1 placeholder — cloud HSM, osslsigncode, or
# Azure Trusted Signing:
$env:REGENT_SIGN_COMMAND = "trusted-signing-cli -e https://eus.codesigning.azure.net -a <account> -c <profile> %1"

powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-setup.ps1
```

One signed build covers everything we ship: NSIS signs the uninstaller it
writes with the same command, and the GUI `uninstall.exe` is a byte copy of the
signed installer.

`tauri.conf.json` already sets `digestAlgorithm: "sha256"` and a `timestampUrl`.
Set `tsp: true` if your CA uses an RFC-3161 timestamp server (SSL.com does).

**Where to get a certificate** (researched 2026-07-16):

| Route | Cost | Notes |
| --- | --- | --- |
| **Azure Trusted Signing** | ~$9.99/mo | Microsoft-managed, no hardware token, open to individual developers (public preview). What Nous Research signs hermes with. Recommended. |
| Classic OV/EV cert | ~$100–400/yr | DigiCert / Sectigo / SSL.com; EV ships a USB token. |
| SignPath Foundation | free | **Applied, outcome uncertain:** their terms require every component to be OSI-licensed; the Chorus display font is freeware (free commercial *use*, not an OSI licence), documented candidly in `LICENSE-chorus.txt`. If a strict reading rejects it, Azure is the fallback. CI integration is already wired in `.github/workflows/installer.yml`, dormant until `SIGNPATH_API_TOKEN` + `SIGNPATH_ORGANIZATION_ID` exist. |

Signing grants a verified publisher name, not instant SmartScreen trust —
reputation accrues per identity, faster on a Microsoft-issued cert.

### macOS (Developer ID + notarization)

Requires a paid Apple Developer account. Set `bundle.macOS.signingIdentity` to
your Developer ID Application identity; `hardenedRuntime` is already on by
default, which notarization requires. Add an `entitlements` file if the app ever
needs a capability the hardened runtime blocks, and `providerShortName` if your
Apple ID belongs to more than one team.

Notarization credentials are passed as environment variables to
`bun run tauri build`. Check the Tauri v2 signing docs for the current variable
names before wiring CI — they are read by the bundler binary that ships with
`@tauri-apps/cli`, not by anything in this repo, so this file would go stale.

### Linux

AppImages are not signed. Nothing to configure.

## Gotchas

- **`payload/README.md` is committed on purpose.** The `payload/**/*` resource
  glob fails the build if nothing matches, so an empty `payload/` would break
  `tauri dev` for anyone who has not run the build script yet. The README keeps
  the glob satisfied.
- **`src-tauri` opts out of the root workspace** (empty `[workspace]` table),
  same as the desktop app: Tauri wants edition 2021 and a self-contained dep
  graph, and the repo root is edition 2024.
- **The desktop app is bundled as a bare executable**, not as an `.exe`
  installer or `.app`. `build-payload` passes `--no-bundle` — nesting an
  installer inside an installer would be pointless.
- **WebView2** is handled by `webviewInstallMode: embedBootstrapper`, so
  Windows machines without it get it during install.
