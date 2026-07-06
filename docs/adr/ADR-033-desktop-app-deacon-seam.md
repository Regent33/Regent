# ADR-033 — Desktop app: Tauri shell over the deacon's stdio JSON-RPC

**Status:** accepted (2026-07-06) · **Context:** `src/regent-app/Desktop/`

**Context.** The Regent Desktop app (Next.js static export inside Tauri v2) needs a
backend seam for chat, code, settings, and Butler voice. Candidates considered:
`regent-web` HTTP (rejected — it is a thin call-page *client*, not a backend), the
deacon's REST ingress (rejected — deny-by-default webhook surface, no session
streaming), embedding `regent-deacon` as a lib crate (rejected — couples the shell to
internals built around a stdio loop).

**Decision.** The Tauri Rust core spawns `regent-deacon` as a hidden child process and
speaks its newline-delimited JSON-RPC 2.0 stdio protocol — the same transport
`regent-cli` (spawn.ts) and `regent-voice-server` (infra/deacon.rs) already use. One
validated `deacon_request` invoke command + a `deacon-event` Tauri event forward the
protocol to the webview; streamed events keep `session_id` and the UI filters on it.
Butler voice bypasses the deacon bridge: the webview calls the existing
`regent-voice-server` HTTP API (`/call/*`) directly, CSP-allowed for that port only.
The webview holds no shell/fs capability; all process management lives in Rust.

**Consequences.** Every Regent front-end shares one wire protocol; new deacon methods
appear on desktop without shell changes. The desktop resolves prebuilt
`regent-deacon`/`regent-voice-server` binaries (never builds them). The Rust bridge
(`DeaconRpc` port) is a second Rust copy of the RPC client — candidate for a shared
crate if it drifts.
