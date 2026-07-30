//! Runtime knobs: context compaction, memory, cron, autonomous board, the HTTP
//! ingress, and tool exposure. Every section defaults so a minimal config.yaml
//! still boots; `deny_unknown_fields` makes a typo a hard error, not a silent
//! default.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextConfig {
    pub max_tokens: u32,
    /// f64 (not f32): must round-trip config.get/config.set JSON exactly —
    /// an f32 0.85 reads back as 0.85000002… in the settings UI.
    pub trigger_fraction: f64,
    pub protect_last_n: usize,
    /// SPL P3 (§3.8): once a tool result is this many user turns old, its content
    /// is replaced by a stub, shrinking history and deferring compaction. Batched
    /// behind a token floor; `protect_last_n` is honored absolutely.
    pub prune_after_turns: usize,
    /// Per-model context-window overrides (ADR-038 P0a): exact model id →
    /// window in tokens. Wins over the built-in family table, which only knows
    /// documented cloud families and goes stale as providers grow their
    /// windows — this is how a user keeps up without a release, and the ONLY
    /// way to size a local model (an Ollama window is a server setting, not a
    /// model-family fact). Too LOW merely compacts early; too HIGH overflows
    /// requests — when unsure, undershoot. Empty (default) = table + fallback.
    pub windows: std::collections::HashMap<String, u32>,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 200_000,
            trigger_fraction: 0.85,
            protect_last_n: 10,
            prune_after_turns: 5,
            windows: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryConfig {
    /// Base directory for skills, cron jobs, and state.db.
    /// Tilde is expanded at runtime.
    pub home: String,
    /// Enable the local ONNX semantic (vector) lane of memory retrieval.
    /// When true (default) the deacon loads the embedding model on boot and
    /// fuses vector recall with FTS + graph; if the model can't load, memory
    /// degrades to FTS + graph rather than failing.
    pub embeddings: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            home: "~/.regent".to_owned(),
            embeddings: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CronConfig {
    pub tick_interval_secs: u64,
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoardConfig {
    /// Opt-in: when true, the deacon auto-runs `todo` tasks on the default
    /// board through an agent. **Off by default** — autonomous execution (and
    /// its token spend) is never enabled silently. Boards still default to
    /// `human` review, so even when enabled nothing self-completes unless a
    /// board's policy says so.
    pub enabled: bool,
    /// Seconds between dispatch ticks.
    pub tick_interval_secs: u64,
    /// Most tasks dispatched per tick (so one busy board can't starve the loop).
    pub max_per_tick: usize,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tick_interval_secs: 15,
            max_per_tick: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpConfig {
    /// Opt-in REST ingress (`/health` + bearer-auth `/v1/chat`). **Off by
    /// default** — the deacon's primary transport is stdio JSON-RPC.
    pub enabled: bool,
    /// Listen address. Defaults to loopback so it is never world-exposed by
    /// accident; bind to `0.0.0.0:..` deliberately to face the network.
    pub bind: String,
    /// Bearer token required on `/v1/chat`. Empty disables the listener
    /// (deny-by-default — never serve the REST surface unauthenticated).
    pub token: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: "127.0.0.1:7878".to_owned(),
            token: String::new(),
        }
    }
}

/// Tool exposure. `disabled` names are filtered out of every session's catalog
/// (`tools disable <name>`), so the model never sees them. `deferred` names
/// stay executable but their schemas are withheld from every request until
/// loaded via `load_tools` — the token-efficiency lever: rare tools stop
/// costing their full schema on every model call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolsConfig {
    pub disabled: Vec<String>,
    pub deferred: Vec<String>,
    /// Adaptive tool tiering (SPL §3.5): when on, tools that do not earn their
    /// keep over the last 30 days are auto-deferred at session build (schemas
    /// withheld, still executable + loadable via `load_tools`) — catalog
    /// growth becomes pay-when-used instead of a per-turn tax.
    pub auto_tier: bool,
    /// How much use earns residency, as a share of assistant turns in the
    /// window. A tool used in fewer than this share of turns is deferred.
    ///
    /// This used to be "any use at all", which is not a threshold — one call in
    /// a month bought thirty days of full-schema residency on every turn.
    /// Measured on a real store (4,152 turns / 30 days): 41 tools qualified, so
    /// effectively nothing deferred, and `create_document` alone spent ~6.2M
    /// tokens to serve 21 calls — about 296k tokens per use, against a
    /// `load_tools` hop costing tens.
    ///
    /// A share rather than a count because it self-calibrates: on a quiet store
    /// the bar is a use or two and nothing defers, which is correct — there is
    /// no cost problem yet. `0.0` (the default) is the any-use rule: defer only
    /// tools with no recorded use at all.
    ///
    /// **Why the default is `0.0` and not `0.01`.** It shipped at `0.01` and that
    /// was a regression, reported the same day. On the owner's store — 4,224
    /// assistant turns in the window — 1% is a 42-use bar, which hid 23 of the 42
    /// tools actually in use, `create_document` among them at 21. Asking for a
    /// PPTX then produced "I'll create the presentation" and a turn that ended
    /// with no tool call at all.
    ///
    /// Deferral is only safe if the model reliably calls `load_tools` to pull the
    /// schema back. Weak models do not, and the reveal-on-stuck net in
    /// `output_check` does not save them: it triggers on EMPTY assistant content,
    /// and a model that says "let me do that now" in prose has non-empty content,
    /// so the turn ends looking like a success.
    ///
    /// The measurement behind this is still right — `create_document` really did
    /// cost ~6.2M tokens over 30 days to serve 21 calls. It justifies the
    /// mechanism and the knob, not an aggressive default flipped on for everyone,
    /// which is exactly what the plan's own gate for this work said not to do.
    /// Raise it deliberately, with `tools.pinned` covering anything the model
    /// cannot afford to lose.
    pub auto_tier_min_share: f64,
    /// Never auto-deferred (the §3.5 safety valve): the core loop the model
    /// must always see schemas for, regardless of recent usage.
    pub pinned: Vec<String>,
    /// ADR-038 P1 kill-switch: route plain chat sessions to the `light`
    /// prompt profile (~5 pinned tools + the deferred index; escalates to
    /// full one-way on `load_tools`/`code_task`/`delegate_task`). `false` =
    /// every session builds the full profile, byte-identical to pre-P1.
    pub light_profile: bool,
    /// Lifecycle shell hooks (gap S7), observe-only and fire-and-forget: a
    /// command spawned when any tool dispatch starts. It sees
    /// `REGENT_HOOK_EVENT` / `REGENT_HOOK_TOOL` / `REGENT_HOOK_PAYLOAD`.
    /// Empty = off.
    pub hook_tool_start: String,
    /// Same, spawned after every tool dispatch completes.
    pub hook_tool_complete: String,
    /// Auto mode: approve every tool gate (dangerous terminal commands,
    /// file move/copy/delete, computer_use, the coding harness) without
    /// prompting. Live: `config.set tools.auto_approve` flips open sessions
    /// too, not just new ones. Equivalent to `REGENT_AUTO_APPROVE=1`, but
    /// toggleable from the app/CLI. Default OFF — never auto-approve by
    /// accident.
    pub auto_approve: bool,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            disabled: Vec::new(),
            auto_tier: true,
            // 1% of turns. Deliberately not aggressive: the pinned list below
            // already holds the whole working loop, so this only reaches tools
            // a session genuinely rarely touches — and on a quiet store 1% is a
            // use or two, so a new install defers nothing.
            auto_tier_min_share: 0.0,
            // Sized against the P4 acceptance ceiling (model-facing catalog
            // ≤2.5k tokens): the core loop only — everything else (incl.
            // glob/memory/code_task) earns residency through recorded use
            // within a day of real work, and stays directly callable +
            // loadable meanwhile.
            pinned: [
                // Files + terminal: the working loop.
                "read_file",
                "write_file",
                "file_edit",
                "apply_patch",
                "glob",
                "ls",
                "search_files",
                "terminal",
                // Web: search without fetch can't read what it found.
                "web_search",
                "web_fetch",
                // Recall + the present moment: "what did we discuss before"
                // and "what's the exact date/time" are first-message questions.
                "memory_search",
                "session_search",
                "session_list",
                "current_time",
                // Direct media intent must never take the skill-loader detour.
                "play",
                // "pull up <site>" is a direct action too — a tiny schema, so
                // keeping it resident costs little and avoids a load_tools round
                // trip weak models don't make.
                "open_url",
                // Registered only when desktop control is enabled. Screen
                // questions and named window/tab actions are direct intents;
                // hiding this behind load_tools made Butler deny it could see.
                "computer_use",
                // "can you see me?" is the camera twin of the screen intent
                // above. Deferred, these two forced the weak Butler driver
                // into a reasoning-only dead-end (no callable capture/vision
                // tool on turn 1), which fires reveal_all_deferred and busts
                // the tier0 prompt-prefix cache for EVERY remaining turn of the
                // vision exchange (observed 2026-07-18). Pinned, they ride the
                // stable cached prefix instead: one first-turn cost, then cached
                // — no mid-session reveal, no cache bust. camera_capture's own
                // description points the model straight at vision_analyze, so
                // both must be resident for the camera path to complete without
                // a stuck turn.
                "camera_capture",
                "vision_analyze",
                // The skills index instructs "load it with skill_view(name)"
                // and overflows to "skills_list shows all" — both must exist
                // the moment the index says so.
                "skills_list",
                "skill_view",
                // THE coding path (ADR-027): auto-routing dies if the model
                // can't see code_task.
                "code_task",
            ]
            .map(String::from)
            .to_vec(),
            // Rare, schema-heavy tools; override with `tools.deferred: []`.
            deferred: [
                "manage_keys",
                "image_generation",
                "video_analyze",
                "control_app",
                "kanban",
                "update_persona",
                "skill_manage",
                "move_file",
                "copy_file",
                "delete_file",
                "send_file",
                // camera_capture + vision_analyze were here (the next-biggest
                // schemas a typical chat turn doesn't need up front), but a
                // Butler "can you see me?" turn DOES need them on turn 1 —
                // deferred, they triggered the reveal→cache-bust cascade. Now
                // pinned next to computer_use above.
                "delegate_task",
                "send_message",
                // Office/PDF extraction — big schema payoff only when a
                // document actually shows up.
                "read_document",
                // Document generation — same reasoning, write side.
                "create_document",
                // The everyday toolset: real daily utility, but none of them
                // belong in every request's schema bill.
                "calc",
                "convert",
                "date_calc",
                "dictionary",
                "qr_code",
                "random_gen",
                "reminder",
                "sun_moon",
                "weather",
                "world_time",
            ]
            .map(String::from)
            .to_vec(),
            light_profile: true,
            hook_tool_start: String::new(),
            hook_tool_complete: String::new(),
            auto_approve: false,
        }
    }
}
