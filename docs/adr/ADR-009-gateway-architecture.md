# ADR-009: Gateway = pull adapters + contract-isolated runner; auth before everything

**Status:** Accepted

**Context:** M5 needs the messaging surface with the Hermes invariants: default-deny auth with
DM pairing, bypass commands (`/stop`, `/approve`, `/deny`) reaching the runner while a turn is
in flight, approval-over-chat with deny-on-timeout, and one shared command registry.

**Decision:** `regent-gateway` keeps the runner platform-agnostic AND agent-agnostic via two
domain contracts: `PlatformAdapter` (pull model — `next_event`/`send`; Telegram long-poll is the
first impl, wire codecs unit-tested as pure functions) and `ConversationHandler` (the agent side;
the composition-root impl maps session keys to per-chat `Agent`s with `reset_interrupt` per
turn). Dispatch order is fixed: **auth → commands → conversation**; unknown users get exactly
one capability (redeeming a one-time pairing code); one running turn per session with an explicit
"busy — /stop" reply instead of silent queueing. `ApprovalRouter` + `ChatApprovalHandler`
implement the dangerous-action gate over chat (oneshot per chat, timeout = deny). `/cron`-style
deliveries and message queueing/interrupt-redirect are deferred to the daemon milestone.

**Consequences:** All exit criteria are proven against a mock adapter (round-trip, pairing,
/stop bypass + cancel, approve + timeout-deny); the live Telegram round-trip needs only a bot
token (`regent-gateway` bin). Webhook/REST adapters land with the JSON-RPC daemon, where an HTTP
listener already exists. The runner crate's lib stays free of regent-agent imports; only the bin
(composition root) wires agents.
