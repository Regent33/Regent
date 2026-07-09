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
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_tokens: 200_000,
            trigger_fraction: 0.85,
            protect_last_n: 10,
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
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            disabled: Vec::new(),
            // Rare, schema-heavy tools; override with `tools.deferred: []`.
            deferred: [
                "manage_keys",
                "image_generation",
                "video_analyze",
                "play",
                "control_app",
                "kanban",
                "update_persona",
                "skill_manage",
                "move_file",
                "copy_file",
                "delete_file",
                "send_file",
                // Measured 2026-07-09 (tests/token_budget.rs): the next-biggest
                // schemas a typical chat turn doesn't need up front.
                "camera_capture",
                "vision_analyze",
                "delegate_task",
                "send_message",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}
