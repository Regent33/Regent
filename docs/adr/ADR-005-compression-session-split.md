# ADR-005: Compression = child-session split, never history mutation

**Status:** Accepted

**Context:** Long conversations exceed the context window. Hermes compresses by summarizing the
middle, protecting the newest N messages, and splitting the session (lineage via
`parent_session_id`) — because mutating a live session's rows would corrupt resume, search, and
the reproducibility ledger.

**Decision:** `Agent::maybe_compress` (preflight, threshold = `trigger_fraction` ×
`max_context_tokens`, estimate = chars/4) summarizes the head via one provider call, rebuilds the
transcript as `[summary-as-user, optional assistant bridge, verbatim tail]` through the
invariant-checking `Transcript`, writes it all into a **new child session** carrying the same
frozen system prompt, and ends the parent with reason `compressed`. The tail split walks backward
over tool results so an assistant is never separated from its results.

**Consequences:** Parent sessions are immutable history; lineage is walkable; resume of a child
works unchanged (stored prompt wins). Sticky provider failover (`FallbackChat`) and per-turn
outcome rows (`turns` table) complete the M1 reliability story. Auxiliary summarizer models and
memory-flush-before-compression arrive with the memory milestone (M2).
