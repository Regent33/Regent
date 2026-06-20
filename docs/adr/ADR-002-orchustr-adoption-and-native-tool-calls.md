# ADR-002: Selective Orchustr adoption; Regent owns the tool-calling provider layer

**Status:** Accepted

**Context:** Orchustr is consumed as a local path dependency (`../Orchustr/orchustr`). Its
`or-conduit::ConduitProvider` contract is text-only — `or-sentinel`'s ReAct topology parses tool
intents out of response text. Hermes parity requires native OpenAI-style `tool_calls` (parallel
calls, `tool_call_id` plumbing, strict role alternation). `or-forge::ForgeRegistry` cannot
enumerate registered tool definitions publicly, which schema-building needs.

**Decision:** Adopt Orchustr crates where they fit today: `or-core` (RetryPolicy, BackoffStrategy,
TokenBudget, TokenUsage) now; `or-forge`/`or-mcp` at the MCP milestone; `or-recall`, `or-loom`,
`or-colony`, `or-lens` at their milestones. The chat contract with native tool calls lives in
`regent-providers`; the tool manifest lives in `regent-tools::ToolCatalog`.

**Consequences:** Core works against real provider wire formats from day one. Long-term path is
upstreaming a tool-call-native contract (and registry enumeration) into Orchustr, then shrinking
the Regent layer.
