# ADR-001: Tokio-native Rust orchestration (no Node plane)

**Status:** Accepted (user decision, 2026-06-11)

**Context:** Proposal v1 placed orchestration in a Node.js/TypeScript daemon over an FFI seam to
Rust. The user directed: use Tokio instead of Node. The local Orchustr workspace is Rust-first and
Tokio-backed, making the host-language seam unnecessary.

**Decision:** Regent is two planes, not three: a Rust/Tokio core (orchestration + execution in one
cargo workspace, Orchustr crates linked natively) and a Go CLI speaking JSON-RPC to it later.
TypeScript is no longer a runtime plane.

**Consequences:** No FFI/NAPI seam to build or debug; one async runtime; the agent loop, providers,
tools, and store share types directly. Gateway/platform adapters will be Rust crates (edge cost
moves from npm ecosystem to crates.io). Proposal v1 §2/§3 superseded by the amendment in v1.1.
