# ADR-031 — Deferred toolsets: schemas fetched only when needed

**Context.** Every request pays for every tool's full JSON schema (~30 tools),
a large share of the 10–15k tokens/request, while most tools go unused in a
typical session. Quality rules: no prompt or constitution cuts.

**Decision.** `ToolCatalog` gains deferred tools: names listed in config
`tools.deferred` (default: manage_keys, image_generation, video_analyze, play,
control_app, kanban, update_persona, skill_manage, move/copy/delete_file,
send_file) stay registered and executable, but their schemas are withheld
from `definitions()`. A `load_tools` tool — whose description carries a
one-line index of what's loadable — returns full schemas on demand and
activates them for subsequent requests; a direct call to a deferred tool also
activates it. Skills-index hooks are capped at 140 chars (body still loads
whole via `skill_view`).

**Consequences.** Several thousand tokens saved per request with zero
capability loss (one load round-trip on first use of a rare tool). Activation
busts the prompt cache once per load — the intended trade.
