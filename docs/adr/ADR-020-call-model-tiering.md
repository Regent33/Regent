# ADR-020: Call model tiering — fast model for quick replies, main model for thinking

**Status:** Proposed — 2026-06-22

**Context:** A spoken turn is latency-critical — a multi-second pause in a voice/video
call feels broken, whereas the same pause is fine in text chat. The main model is tuned
for reasoning quality, not sub-second response. The master prompt's cost/latency rule
already says: cheap/fast model for the quick path, expensive model only for
reasoning-heavy steps.

**Decision:** The call path **tiers the LLM**. A configurable **fast model**
(`speech.call.fast_model`, e.g. **Gemini 3.1 Flash Lite** or any `*-flash`/lite model)
answers conversational turns and acknowledgements — the snappy default voice. The
**main model** (`model.default`) handles reasoning-heavy turns. **Escalation:** the fast
model runs first; if it requests a tool or signals it needs to think, the pipeline plays
a short filler ("let me look into that…") and re-runs the turn on the main model.
Barge-in (VAD) can interrupt either model's reply. `call.fast_model: ""` disables
tiering (everything uses the main model). Tiering serves real-time calls (V4) primarily
but also tightens turn-based voice latency (V1). It applies **only** to the call/voice
path — text chat keeps using the main model directly.

**Consequences:** Calls feel responsive without sacrificing depth on hard turns; spend
drops because most spoken turns hit the cheap model. The fast model is provider-agnostic
(it rides the same `ChatProvider` contract), so "any flash model" is a config change.
Escalation adds one extra call on the turns that need thinking — paid for by the filler,
not silence.
