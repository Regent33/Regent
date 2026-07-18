# ADR-039: Vision model routing precedence

**Status:** Accepted — 2026-07-18

**Context:** Vision tools consume `REGENT_VISION_*`, while Settings stores an
optional route in `speech.vision`; stale environment values made intent unclear.

**Decision:** A named `speech.vision.provider` and model are authoritative and
resolve through `config.providers`. `auto` restores the pre-existing behavior:
manual `REGENT_VISION_*` values win, otherwise Regent derives the route from the
primary chat model. Invalid, incompatible, or unavailable explicit providers
fall back to Auto with a warning. Keyless provider routes omit authorization.

**Consequences:** Settings applies to the next tool call without new RPCs or
schemas, switching back to Auto is reversible, and existing setups are stable.
