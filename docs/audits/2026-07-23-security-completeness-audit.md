# Security, token-efficiency, and installer completeness audit — 2026-07-23

**Scope:** current tree after the 2026-07-23 fixes; runtime evidence from
`~/.regent/logs/regent.log.2026-07-23` and `state.db` (read-only, WAL-aware).
**Rule:** “secure” below means a verified control exists, not a guarantee.

## Executive answer

- **Today’s sampled sessions were not token-efficient.** The failed deck session
  accumulated **123,041 input tokens / 6 calls**; the “open the last song” turn
  accumulated **244,898 / 14**, including eight `session_search` calls. The main
  causes were repeated full-context sends, three large raw web-search payloads,
  a 50-hit session search, and one recovery that revealed 36 deferred schemas.
- **The store, gateway, deacon, and tools have strong default controls**, especially
  for external ingress. They are not risk-free: local sessions are not sandboxed
  by default, state is not encrypted at rest, and prompt injection remains a
  model-level risk.
- **The matching CLI and GUI installer/uninstaller pairs are complete on Windows
  and Linux for their intended paths.** macOS has no signed GUI DMG yet, and Regent
  does not yet have a coordinated multi-component updater/rollback system.

## 1. What the logs proved

### Comparison incident

At 17:02:16 the user asked for a Ferrari/Lamborghini practicality explanation.
The agent called `create_document` with diagram-only `layout:"compare"` and
`items/points`, then spoke 43 seconds later. `Slide` ignored the unknown fields,
and the invalid layout became a title-only section slide.

Fixed by:

- prompt schema v4: explanations use the inline visual channel unless a file is
  explicit ([system.rs](../../src/crates/regent-agent/src/domain/prompts/system.rs));
- strict nested document fields and supported-layout validation before any write
  ([model.rs](../../src/crates/regent-tools/src/infra/create_document/model.rs),
  [validate.rs](../../src/crates/regent-tools/src/infra/create_document/validate.rs)).

The renderer itself remains dynamic: content-derived palettes and per-slide
layout inference still drive PPTX/PDF/DOCX/XLSX. No static “comparison template”
was added.

### Cutoff and token incident

At 17:06:54 a reasoning-only recovery revealed 36 tool schemas. After three large
search results, NVIDIA emitted no first SSE byte before reqwest’s fixed 60-second
read timeout. Reqwest displayed that timeout as `error decoding response body`.
The fallback was another model on the same NVIDIA host and failed too.

Fixed by:

- removing the extra 60-second read timeout while retaining the 10-second connect
  and 120-second total request bounds ([openai_compat.rs](../../src/crates/regent-providers/src/infra/openai_compat.rs));
- stable timeout/connection classification and actionable UI text
  ([http.rs](../../src/crates/regent-providers/src/infra/http.rs),
  [turn_errors.rs](../../src/crates/regent-deacon/src/application/dispatcher/prompt_ops/turn_errors.rs));
- 500-character web/search snippets and a maximum 20 session hits
  ([web_search.rs](../../src/crates/regent-tools/src/infra/web_search.rs),
  [session_tools.rs](../../src/crates/regent-tools/src/infra/memory_tools/session_tools.rs)).

**Residual efficiency risk:** `reveal_all_deferred` remains because it fixes a
separately reproduced weak-model tool-starvation failure. It is a rare but real
cache/context reset; targeted recovery needs its own model-regression evaluation.

## 2. Security by subsystem

| Subsystem | Confirmed controls | Residual / unverified |
|---|---|---|
| `regent-store` | SQLite WAL, foreign keys, `BEGIN IMMEDIATE`, bounded busy retries, separate read-only connection ([db.rs](../../src/crates/regent-store/src/infra/db.rs)); additive schema reconciliation | `state.db` is not encrypted at rest; one writer is a throughput ceiling; newer-than-runtime schemas warn rather than downgrade |
| Gateway/webhooks | Platform signature verification, default-deny sender authorization/pairing, per-user rate limiter before turns; external sessions are jailed | Full cryptographic/replay review of every adapter was not repeated; platform account compromise remains outside Regent |
| Deacon | Primary CLI/app transport is local stdio; optional HTTP refuses to start without a bearer token and defaults to loopback ([http_serve.rs](../../src/crates/regent-deacon/src/application/http_serve.rs)) | Deliberately binding externally expands the threat surface; token custody is operator responsibility |
| Tools | Path jail canonicalizes existing prefixes; external sessions always jailed; SSRF validation pins the connected IP and checks redirects/body caps ([net.rs](../../src/crates/regent-tools/src/infra/net.rs)); child commands strip secret-looking env vars; mutations cross approval gates | Local sessions are unjailed unless `REGENT_SANDBOX=1` ([sandbox.rs](../../src/crates/regent-tools/src/infra/sandbox.rs)); full-control voice is intentionally high privilege |
| Voice | Default voice denies every gated mutation; read-only screen/camera vision still works; `REGENT_VOICE_FULL_CONTROL=1` is explicit | Screen/web content is untrusted input; enabling full control accepts prompt-injection and misrecognition risk |
| Skills/agents | Named agents can have tool allow-lists; plan/code phases physically restrict tools; approvals and the tool jail remain below prompts | A skill is instruction, not a sandbox or signature. Install only trusted skills; prompt/tool-output injection defenses are not complete |
| Dependencies | `cargo deny` rejects unknown registries/git sources and unapproved licenses ([deny.toml](../../deny.toml)) | `RUSTSEC-2023-0071` (`rsa` timing side channel) is explicitly accepted because no fixed release exists; two unmaintained transitive advisories are tracked |
| Installer | Regent archives require workflow-generated SHA-256 sidecars; current v0.1.1 assets have pinned rollout digests; Windows ffmpeg is version/hash pinned; GUI builds get GitHub provenance attestations; hostile install paths are escaped | A checksum shares GitHub as its trust root and is not a signature. Core release archives are not yet independently signed/attested. Source fallback trusts the checked-out GitHub source and local toolchain |

## 3. Prompt/constitution invariant

Tool deferral never applies to prompts. SYSTEM_PROMPT and CAPABILITIES are Tier 0
segments and are never trimmed. The constitutional core is synced at boot,
renders first in the persona block, and persona is trimmed last. A regression
test now pins those properties in
[tests/build.rs](../../src/crates/regent-deacon/src/application/session_manager/tests/build.rs).
The full constitution remains pinned in graph memory as trusted section nodes.

## 4. Installer/uninstaller completeness

### One-line CLI install

- Windows: verified archive, `%USERPROFILE%\.regent\bin`, PATH-safe shim,
  optional pinned ffmpeg, automatic setup when interactive.
- macOS/Linux: verified archive, `~/.regent/bin`, `~/.local/bin/regent`, automatic
  setup through `/dev/tty`; ffmpeg uses package-manager guidance.
- Matching uninstallers stop processes, remove their binaries/link/PATH pin, and
  preserve data unless purge is explicit.

### GUI Setup

- Windows: app + CLI/deacon, Start Menu search entry, optional Desktop shortcut,
  Apps & features entry, deacon pin, inverse GUI uninstall.
- Linux: AppImage installer, app-menu `.desktop` entry, install detection, inverse
  uninstall script.
- macOS: **incomplete as a GUI release** until Apple Developer ID signing exists;
  use the one-line installer.

The one-line uninstaller is not a universal GUI uninstaller; users must use the
matching uninstall path. A coordinated updater/rollback and mixed-version
compatibility policy is the next planned workstream.

## 5. Priority after this pass

1. Design the cross-component updater with version/schema compatibility checks,
   atomic replacement, rollback, and mixed-version refusal.
2. Add independent signing/attestation verification for core release archives.
3. Evaluate targeted deferred-tool recovery against weak models before replacing
   `reveal_all_deferred`.
4. Expand prompt-injection tests at web/tool/skill boundaries.
5. Re-review every webhook adapter’s replay window and crypto implementation.
