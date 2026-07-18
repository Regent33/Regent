# Plan — Vision model selector in Settings → Model (provider + model, live)

**Date:** 2026-07-18 · **Status:** implemented (local-provider execution follow-up requested separately)

## 1. Problem

Settings → Model has no Vision entry. The vision/video tools (`vision_analyze`,
`video_analyze`, `read_document`'s model-direct rung) pick their model from
`REGENT_VISION_MODEL` / `REGENT_VISION_BASE_URL` / `REGENT_VISION_API_KEY` env
vars, which the deacon auto-derives **from the primary chat model** at boot and
on every `config.set`/`env.set` (`export_vision_route`,
[routing.rs:47](../../src/crates/regent-deacon/src/bin/regent-deacon/routing.rs#L47)).
There is no UI to point vision at a *different* provider/model than chat.

Config fields for exactly this already exist — `speech.vision.provider` /
`speech.vision.model`
([speech.rs:109](../../src/crates/regent-deacon/src/domain/config/speech.rs#L109))
— but **nothing consumes them at runtime today** (dead config; only
`input_mode` is reported by `speech_factory`). This is also the open
"vision provider-key derive" item from the 2026-07-18 session.

## 2. Design

Single source of truth: **`speech.vision.{provider,model}` in config.yaml.**
The deacon derives base URL + API key from the named provider entry (same
name-keyed `config.providers` map the chat picker uses) and exports the
`REGENT_VISION_*` env vars the tools already read. No tool code changes.

### Precedence (documented in code + ADR)

| speech.vision.provider | Result |
|---|---|
| `"auto"` or `""` (default) | Today's behavior, unchanged: derive from primary chat model; a user-set `.env` `REGENT_VISION_*` var wins over the derived value. |
| a name from `config.providers` | Explicit choice: export that provider's base URL + key + `speech.vision.model` **unconditionally** (still tracked in the `REGENT_VISION_AUTO` marker so switching back to auto restores the old behavior cleanly). Explicit config outranks a stale hand-set `.env` var — it is the newer, deliberate user intent from Settings. |

`speech.vision.model` empty while provider is explicit → use that provider's
first catalog model? **No — keep it dumb:** the UI always writes both fields
together, so an explicit provider always carries a model. Backend treats
provider-set-but-model-empty as auto (logged warn).

### Liveness ("live merge")

- `config.set` already fires the deacon's live-reload hook
  ([wiring.rs:19](../../src/crates/regent-deacon/src/application/dispatcher/wiring.rs#L19))
  → `routing_from(cfg)` → `export_vision_route` re-runs → env vars updated in
  the running deacon → the very next `vision_analyze` call uses the new model.
  **No restart needed for the app's deacon.**
- Known limitation (unchanged from today, noted in changelog): the
  voice-server's own long-lived deacon reloads only on *its* config.set; a
  Settings change applies there on its next start. Same story as main-model
  changes.

### Edge cases

- **Anthropic provider selected:** `openai_style_base` returns `None` (wire
  shape not OpenAI-compatible). Backend falls back to auto behavior and logs a
  warn. UI hint text mentions vision needs an OpenAI-compatible provider.
- **Keyless provider (local ollama):** export base + model, no
  `REGENT_VISION_API_KEY`. `vision_analyze` currently *requires* a key —
  pre-existing limitation, out of scope here.
- **Custom model id:** same "Custom…" free-text affordance as the main picker.
- **Provider named in config but key env unset:** export nothing for it (fall
  back to auto + warn) — mirrors `ProviderRegistry::provider_for`'s
  MissingKey semantics.

## 3. File-by-file changes

### Backend (Rust)

1. **`src/crates/regent-deacon/src/bin/regent-deacon/routing.rs`**
   — `export_vision_route` gains access to the config (change
   `routing_from(cfg)` to pass `&cfg.speech.vision` + `&cfg.providers` through;
   simplest: `export_vision_route(routing, cfg)`). New first branch: explicit
   provider → look up `ProviderSpec` by name → base =
   `openai_style_base(spec.kind, spec.base_url)`, key = env var named by
   `spec.api_key_env` (empty `api_key_env` = keyless → skip key export), model
   = `speech.vision.model` → export unconditionally, record in marker.
   Fall through to existing auto derivation otherwise.
   **Why:** the one seam that already runs at boot + every config/env change.

2. **`src/crates/regent-deacon/src/bin/regent-deacon/routing.rs` (tests, same
   file `#[cfg(test)]`)** — unit tests: (a) explicit vision provider exports
   its base/key/model over a primary-derived route; (b) `auto` keeps the
   old derivation; (c) unset key env → falls back to auto; (d) switching
   explicit → auto restores/clears marker-owned vars. Env-var tests run
   serially (existing pattern in the crate).
   **Why:** the precedence table above is the contract; each row gets a test.

### Frontend (Desktop app)

3. **`src/regent-app/Desktop/features/settings/presentation/VisionModelSection.tsx`
   (new, ~100 lines)** — one row styled like `MainModelPicker`: Provider
   `<select>` (options: **"Auto — follow main model"** sentinel + names from
   `vm.providers`) · Model `<select>` with "Custom…" free-text (same pattern as
   MainModelPicker, minus the key-slot picker — vision reads the provider's
   base key) · Apply button armed on change. Reads current value from
   `cfg.get('speech.vision')`; Apply writes **one** merged object
   `cfg.set('speech.vision', { ...current, provider, model })` so
   `input_mode`/`download_timeout` are preserved (single validated write, no
   partial state). Auto sentinel writes `provider: "auto", model: ""`.
   **Why:** mirrors the LLM picker UX exactly, reuses `useConfig` +
   `useMainModels` — no new viewmodel.

4. **`src/regent-app/Desktop/features/settings/presentation/ModelSection.tsx`**
   — render `<VisionModelSection cfg={cfg} vm={models} />` in a new
   "Vision model" block under the Auxiliary section.
   **Why:** placement requested: Settings → Model.

5. **`src/regent-app/Desktop/shared/i18n/en/settings.ts`** — add under
   `settings.model`: `visionTitle`, `visionHint` (mentions live-apply +
   OpenAI-compatible requirement), `visionAuto` ("Auto — follow main model").
   **Why:** no hardcoded UI strings (repo rule).

### Docs

6. **`docs/changelogs/CHANGELOG.md`** — entry following repo convention.
7. **`docs/adr/` — new short ADR** — "Vision routing precedence: explicit
   `speech.vision` config > hand-set `REGENT_VISION_*` env > auto-derived from
   primary" (≤15 lines).

## 4. Risks

- **Precedence flip:** a user who set `REGENT_VISION_MODEL` via `voice.set`
  and *then* uses the new selector gets config-wins. That is the intended
  resolution of the ambiguity; ADR records it. Auto mode keeps env-wins, so
  existing setups are untouched until someone actually uses the new selector.
- **Shared contract:** additive only — new config fields already exist in the
  schema; `config.set` whole-file validation unchanged; no RPC changes.
- **Voice-server staleness:** documented, pre-existing.

## 5. Verification

- `cargo test -p regent-deacon` (new routing tests + existing suite green).
- `cargo clippy -p regent-deacon` clean.
- Desktop: `tsc && vite build` green (repo has no test script; vad tests run
  via vitest if configured).
- Manual: pick a vision provider/model in Settings → run `vision_analyze` on
  an image in chat **without restarting** → confirm the call hits the chosen
  model; switch back to Auto → confirm primary-derived route returns.

## 6. Out of scope (deliberate)

- Making `vision_analyze` work keyless (local ollama) — separate fix.
- Key-slot picker for vision — base key only until someone asks.
- `input_mode` UI — config-only knob today, stays that way.
- Video model override (`REGENT_VIDEO_MODEL`) — inherits vision route as today.
