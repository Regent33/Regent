# ADR-041 — Cross-component update notification and staged updates

Status: accepted · 2026-07-24 (Phase 0 implemented; apply phases pending)

**Context:** Regent ships coupled CLI/deacon binaries, a Desktop app, GUI installers,
and source-built gateway/voice components. Versions and protocol constants are copied
across Rust, TypeScript, and Tauri files and have already drifted. In-place replacement
is unsafe on Windows, and unsigned GUI binaries must not update unattended.

**Decision:** publish one compact, additive release manifest after all release assets.
It contains component/protocol versions and per-platform names, sizes, and SHA-256 —
never arbitrary URLs. The deacon performs one cached conditional check per day; CLI,
Desktop, gateway, and voice surfaces consume its additive status/capability response.
One-line installs update into `$REGENT_HOME/versions/<version>` and atomically switch a
stable launcher/current pointer, leaving running versions untouched and rollback-ready.
GUI apps remain installer-driven. Manual updates require verified hashes; opt-in automatic
updates additionally require a detached Ed25519 signature. User data is never replaced.

**Consequences:** rollout is notify-only, then manual one-line updates, then signed opt-in
auto-update. RPC/config/manifest fields stay additive and unknown fields are ignored.
Store-schema bumps require a pre-update backup and explicit rollback policy. macOS GUI
auto-update waits for Developer ID; Windows GUI auto-update waits for active signing.
