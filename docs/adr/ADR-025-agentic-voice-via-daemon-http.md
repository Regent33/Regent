# ADR-025: Agentic voice call via the daemon's HTTP agent

**Status:** Accepted (2026-06-25)

**Context:** The local voice call's "brain" was a plain OpenAI-compatible chat
completion — no tools, memory, or persona. The user wants the call to do what the
CLI agent does ("create a kanban task", "what's on my board?", "open/download X").
The full agent (tool catalog, memory, persona, sandbox) already lives in
`regent-deacon`, which exposes a synchronous `POST /v1/chat` (`{message, session}` →
`{session, reply}`) — but its HTTP listener is off by default and config-driven.

**Decision:**
- **Route call turns through the daemon agent.** The Python speech server spawns a
  `regent-deacon` with its HTTP listener enabled, holds its stdin open (so the
  stdio loop blocks and the process stays alive serving HTTP), and `POST`s each
  turn to `/v1/chat`, persisting the `session` for continuity. Falls back to the
  plain completion when no daemon/model is available. `REGENT_VOICE_AGENT=0` opts
  out for fastest pure-Q&A.
- **Enable HTTP via env, not config.yaml.** New `REGENT_HTTP_ENABLED/BIND/TOKEN`
  overrides in `config_loader` so a launcher can turn on `/v1/chat` without editing
  the user's config. Loopback bind + a per-process random bearer token (generated
  by the speech server) — the REST surface is never unauthenticated.
- The agent reply is synchronous (the tool loop must finish first), so it's fed
  through the same per-sentence Kokoro TTS streaming as the completion path.

**Consequences:** Voice == CLI agent (same `REGENT_HOME`, memory, persona, tools).
Agentic turns are slower than a raw completion (tool loops); that's the trade for
capability, and opt-out exists. Plumbing verified (env-override → HTTP → auth →
agent turn); the live tool-loop needs a real model key. The daemon's stdio-first
design is reused as-is — HTTP is an additive side-listener, lifecycle tied to the
speech server.
