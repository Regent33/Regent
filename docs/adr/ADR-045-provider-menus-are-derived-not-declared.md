# ADR-045: Provider menus are derived, never declared twice

**Date:** 2026-07-31 · **Status:** accepted

## Context

Every provider list in the repo existed at least twice: once where dispatch
happened, once where the menu was rendered. Both copies had drifted.

`BUILTIN_TTS_PROVIDERS` advertised eleven text-to-speech backends — `elevenlabs`,
`minimax`, `gemini`, `piper`, `edge`, `kittentts`, `neutts` — of which four
resolved. The names were ported from Hermes as reserved strings; nothing
implemented them, and `voice.models` rendered them straight into the picker, so
selecting one returned "not wired yet". `LLM_KEYS` (the Settings → API Keys page)
was hand-maintained beside `ProviderKind`, so a provider could be selectable in
the model picker with no row to put its key in. `provider_kind_tests.rs` kept its
own copy of the variant list, which meant the "exhaustive" test silently stopped
covering anything added after it was written.

## Decision

One table per domain is the source of truth, and every list is computed from it.

- **Speech**: `regent_speech::catalog::SPEECH_PROVIDERS` — id, label, blurb, base
  URL, key var, per-kind default model. `builtin_asr_providers()` /
  `builtin_tts_providers()`, the deacon's `resolve_base`/`resolve_key`, and the
  `voice.models` payload all derive from it.
- **Models**: `ProviderKind::ALL` — the API Keys rows (`llm_keys()`) and the tests
  iterate it rather than a copy.
- A row earns its place by being *dispatchable*. A provider that needs a wire the
  adapter cannot build (ElevenLabs' `xi-api-key` + `/text-to-speech/{voice}`,
  Deepgram's raw-body `/listen`) is absent, not listed-and-broken.
- `base_url` always overrides the table, so an unlisted host stays reachable
  without a release; an empty table base means "you must supply one" (Azure,
  RunPod, `custom`).

## Consequences

Adding a provider is a row plus, for models, one line in each `ProviderKind`
match — and it reaches the CLI wizard, the Settings keys page and `voice.models`
automatically. A menu cannot advertise a backend that errors on selection,
because the menu is generated from what dispatches.

Cost: the catalog files run past the 200-line guide (a provider table is data,
and splitting it in half helps nobody), and non-OpenAI-wire providers are now
blocked on `SpeechHttpRequest` gaining custom headers and a raw-bytes body rather
than being quietly listed. That is the intended trade: fewer names, all real.
