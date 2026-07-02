# ADR-032 — Vendor or-core/or-mcp; Regent builds standalone

**Context.** The workspace pinned `or-core`/`or-mcp` to a sibling checkout
(`../Orchustr/...`), so a clone of Regent alone could not build (ADR-002
adopted Orchustr; this fixes the coupling's distribution cost).

**Decision.** Vendor both crates (small: ~12KB + ~40KB of source) into
`src/crates/regent-orchustr-core/` as workspace members with self-contained
Cargo.tomls; the root workspace deps now point at the vendored paths. The
Orchustr repo remains the upstream — updates are copied over per
`regent-orchustr-core/README.md`.
Alternatives rejected: git dependency (requires a hosted remote), crates.io
publishing (heavyweight for two internal crates).

**Consequences.** `git clone` + `cargo build` works with no sibling checkout;
performance identical (same code, same compile). Cost: manual sync when
Orchustr's or-core/or-mcp change — acceptable at their change rate; the
schemars-major lockstep note moves with the vendored copy.
