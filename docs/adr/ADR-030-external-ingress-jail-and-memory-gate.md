# ADR-030 — External ingress is jailed; its memory writes are staged

**Context.** The 2026-07-02 audit found webhook/platform turns ran with host
filesystem access (P1-005) and that the memory write-approval gate
(`staging.rs`, §10.2) existed but nothing routed through it (P1-003).

**Decision.** Keyed sessions (platform webhooks, gateway conversations) always
get a sandboxed `ToolContext` jailed to the workspace — `REGENT_SANDBOX` can
widen the jail to local sessions but can no longer narrow the external one.
The same sandbox flag marks a session untrusted for memory: its `memory add`
is staged as a pending write (7-day TTL, approved via the existing
`memory.pending`/`memory.approve` surface, entry-kind proposals commit through
`add_entry`); `replace`/`remove` from external sessions are refused. Local CLI
sessions stay unsandboxed and write memory directly (single-user machine).

**Consequences.** An injected or unauthorized external turn can neither read
`$REGENT_HOME`/`~/.ssh` nor poison long-term memory silently. W1.1 (per-user
authz) can now decide only *who* runs a turn, not *what a bad turn reaches*.
