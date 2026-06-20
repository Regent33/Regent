# ADR-010: MCP as the plugin surface; terminal backends behind a contract; observer hooks

**Status:** Accepted

**Context:** M6 needed the extension edges: out-of-process tools, remote execution sandboxes,
and an in-process extension seam — without dlopen-style dynamic loading (unsafe, platform-fragile)
and without growing the core tool schema.

**Decision:**
1. **MCP is Regent's out-of-process plugin mechanism** (Footprint Ladder rung 5), via Orchustr's
   `or-mcp`: server tools register namespaced (`{ns}_{tool}`, toolset `mcp-{ns}`) into the same
   catalog; collisions reject loudly; upstream failures return as error JSON. Because or-mcp's
   native-async trait carries no `Send` bounds, the invoker future is **boxed at the concrete
   client site** (`register_mcp_http`) and registration stays non-generic (`McpInvoker`).
2. **Terminal backends behind a domain contract** (`TerminalBackend`): local (default), docker
   (`docker exec`, container workdir), ssh (`BatchMode=yes`) — CLI-shelling, no SDK deps; argv
   builders are pure and unit-tested; selected by `REGENT_TERMINAL_BACKEND`
   (`local | docker:<container>[:workdir] | ssh:<dest>`). Guard/approval/truncation stay in the
   tool; backends only execute.
3. **`DispatchHook`** (before/after every executed dispatch) is the in-process observer seam
   (tracer/audit). Hooks observe, never mutate — mutation-capable middleware would be a new
   decision.

**Consequences:** Third-party capability needs zero core edits (serve an MCP server); sandboxes
are a config switch; in-tree "plugins" are just crates registering into the catalog at the
composition root. Modal/Daytona-style serverless backends and an or-mcp `NexusServer` exposing
Regent's own tools are follow-ups in the parity plan.
