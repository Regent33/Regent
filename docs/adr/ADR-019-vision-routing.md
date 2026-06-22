# ADR-019: Vision routing — text-mode first, native multimodal later

**Status:** Proposed — 2026-06-22

**Context:** The agent must "see" images the user sends (and, on a live video call,
sampled frames). Pixels can't be flattened in the runner the way audio can (audio→text
via ASR is lossless enough; an image→text summary is lossy but still useful). The
kernel `ChatMessage` content model is text-only today, so sending real pixels to a
model is a non-trivial, cache-sensitive change. Hermes already solved this with
`image_routing.decide_image_input_mode` (auto | native | text).

**Decision:** Port Hermes's routing. **`text` mode ships first:** the runner/agent runs
a `vision_analyze` tool on each inbound image (or sampled frame) and prepends the
description to the user text — works with any text model, **zero contract change**.
**`native` mode is a later, separately-gated change** (proposal V5): extend
`ChatMessage` to carry image parts and teach the Anthropic/OpenAI adapters to translate
them, keeping image parts on the **current user message** so the cached prefix stays
byte-stable. **`auto`** picks native when the active model reports vision capability,
else text. Vision targets the main multimodal model or a configured aux model — **no
new vision-model crate** (YAGNI).

**Consequences:** Real-time and turn-based vision both work day one via text-mode with
no provider changes; native (full-fidelity) vision is an opt-in upgrade that cannot
regress prompt-cache stability because it never touches the frozen prefix. The
`vision_analyze` tool stays available to skills/flows regardless of mode (Hermes parity).
