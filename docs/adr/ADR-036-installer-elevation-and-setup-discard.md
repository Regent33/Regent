# ADR-036 — Setup asks for administrator, and discards itself

Status: accepted · 2026-07-16 · supersedes the per-user-only stance in
`src/regent-app/Installer/docs/BUILDING.md`

## Context

Two defects in Regent Setup, both found by running it rather than reading it:

1. NSIS treats Setup as an application to *install*: it unpacked ~80MB and
   registered a second "Regent Setup" row in Apps & features beside the real
   "Regent", both parked for good.
2. The install location was free text on a screen promising "no administrator
   prompt". Typing `D:\Program Files\Regent` failed several stages later with a
   raw PowerShell stack trace, because the install was per-user by design.

## Decision

- **Setup requests administrator at startup**, by relaunching itself with
  PowerShell `-Verb RunAs` (`elevate.rs`) — not with a `requireAdministrator`
  manifest, which would fail NSIS's `RunAsUser` hand-off (a CreateProcess-family
  call: ERROR_ELEVATION_REQUIRED, no prompt) and `tauri dev` alike. A refused
  prompt falls back to the unelevated per-user install.
- **A successful install discards Setup's own directory** (`setup.rs`) by running
  the uninstaller NSIS already wrote, matched by `InstallLocation` and gated on
  the directory not being the install target. Deferred to process exit via
  `Wait-Process`, never a guessed sleep.
- **The location is validated at the boundary** (`check_location`), while the
  field is still on screen.

## Consequences

- One Apps & features row; 0MB parked; `uninstall.exe` stays 9.4MB because the
  payload remains a sibling resource. Embedding it via `include_bytes!` — the
  once-obvious fix — would have made every install park a ~75MB uninstaller,
  worse than the 65MB it set out to save.
- **The install stays per-user while running elevated.** `HKCU` and the user's
  Desktop are correct under Admin Approval Mode, but satisfying UAC with a
  different administrator's credentials writes PATH, the deacon pin, the ARP
  entry and the shortcut to *that* account. Per-machine mode (HKLM, All-Users
  PATH) is the fix if this ever bites; it was deliberately not built yet.
- **"Launch Regent" de-elevates.** A direct child of the elevated installer
  would inherit the admin token, so the Finish screen hands the launch to
  Explorer, which starts the app with the normal desktop token. The deacon pin
  still arrives: `pin_deacon`'s environment write broadcasts WM_SETTINGCHANGE,
  which Explorer honours before spawning children.
- Windows-only. macOS and Linux have no unpacked Setup directory to discard and
  no UAC to ask; both functions are empty stubs there.
