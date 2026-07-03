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

**Update (2026-07-03) — W1.1 landed (P0-001).** The webhook + Discord-interactions
routes now gate every turn on the gateway's `AuthPolicy` (default-deny →
allowlist / paired / allow_all), keyed `platform:user_id`; an unknown sender can
only redeem a one-time pairing code. Auth loading/persistence (`load_auth_snapshot`
/ `persist_auth_snapshot`, atomic tmp+rename) moved into the gateway lib so the
gateway, webhook, and Discord planes share one policy file (`gateway-auth.json`).
The operator config is now platform-agnostic — `REGENT_ALLOW_ALL` +
`REGENT_ALLOWED_USERS` (`platform:id`), with the Telegram vars kept as aliases.
This is a deliberate breaking change: an unconfigured webhook plane is now
closed, not open. *Open follow-up:* per-adapter signature crypto (Slack HMAC,
Twilio, WeChat/Feishu AES, Google Chat JWKS) still wants a dedicated scan — authz
sits on top of `verify_request`, so a weak verifier would undercut it.
