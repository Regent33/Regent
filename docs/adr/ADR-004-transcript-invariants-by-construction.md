# ADR-004: Message alternation enforced by construction, not by review

**Status:** Accepted

**Context:** Providers hard-reject malformed histories (two same-role messages, orphan tool
results). Hermes enforces alternation by discipline across a 12k-LOC loop; violations were a
recurring bug class (e.g. synthetic mid-loop user messages).

**Decision:** `regent_kernel::Transcript` is the only way to build conversation history. Appends
that violate alternation, answer an unknown/already-answered `tool_call_id`, or interleave
user/assistant content while tool calls are pending return typed errors. `Agent::resume` replays
stored history through the same checks, so corrupt sessions fail loudly at resume time instead of
as a provider 400 mid-turn.

**Consequences:** The invariant is unrepresentable rather than reviewed-for. Compression (a future
milestone) must rebuild transcripts through this type, keeping tool-call/result pairs atomic.
