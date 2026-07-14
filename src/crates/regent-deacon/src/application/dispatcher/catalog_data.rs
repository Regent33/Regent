//! Static dispatcher data: the curated model catalog and the RPC command
//! listing. Split from `dispatcher/mod.rs` (file-size rule).

use serde_json::{Value, json};

/// Known Claude models offered by `model.list` (id, display name). `model.set`
/// accepts any string, so custom/self-hosted ids still work — this is the
/// menu, not an allowlist.
pub(crate) fn model_catalog() -> &'static [(&'static str, &'static str)] {
    &[
        ("claude-fable-5", "Claude Fable 5"),
        ("claude-opus-4-8", "Claude Opus 4.8"),
        ("claude-sonnet-4-6", "Claude Sonnet 4.6"),
        ("claude-haiku-4-5", "Claude Haiku 4.5"),
    ]
}

/// The in-chat `/` slash menu, mirrored from the CLI's slash surface
/// (`src/regent-cli/src/app/config/commands.ts::SLASH_COMMANDS`) so the desktop
/// advertises the same set the terminal does. Each row carries an additive
/// `executable` flag: `true` when the deacon has a JSON-RPC path that fulfils
/// the command (the desktop can run it), `false` for controls the UI handles
/// locally or terminal-only tools it can only explain — so the UI routes or
/// explains instead of firing an RPC that would fail. Extra fields are ignored
/// by older clients (they read only `name`/`description`).
pub(super) fn commands_list() -> Value {
    json!([
        // Chat controls.
        {"name": "help",      "description": "List commands and usage",              "executable": false},
        {"name": "new",       "description": "Start a fresh conversation",           "executable": true},
        {"name": "clear",     "description": "Clear the conversation",               "executable": false},
        {"name": "stop",      "description": "Interrupt the running turn",           "executable": true},
        {"name": "approve",   "description": "Approve the pending action",           "executable": true},
        {"name": "deny",      "description": "Deny the pending action",              "executable": true},
        // Session / knowledge.
        {"name": "status",    "description": "Agent + provider status",              "executable": true},
        {"name": "sessions",  "description": "List or resume sessions",              "executable": true},
        {"name": "memory",    "description": "Browse and manage memory",             "executable": true},
        {"name": "learn",     "description": "Teach Regent a new skill",             "executable": true},
        {"name": "skills",    "description": "List available skills",                "executable": true},
        {"name": "insights",  "description": "Show usage insights",                  "executable": true},
        // Board.
        {"name": "kanban",    "description": "View and manage the board",            "executable": true},
        {"name": "agents",    "description": "Manage named persistent agents",       "executable": true},
        // Model / tools / providers.
        {"name": "model",     "description": "Show or set the model",                "executable": true},
        {"name": "providers", "description": "Manage model providers",               "executable": true},
        {"name": "tools",     "description": "List or toggle tools",                 "executable": true},
        {"name": "keys",      "description": "Manage provider API keys",             "executable": true},
        // Persona.
        {"name": "persona",   "description": "Show persona (soul + about)",          "executable": true},
        {"name": "soul",      "description": "Show or edit the soul",                "executable": true},
        {"name": "about",     "description": "Show or edit the about",               "executable": true},
        // Config / ops.
        {"name": "config",    "description": "Show configuration",                   "executable": true},
        {"name": "voice",     "description": "Voice (ASR/TTS): setup, enable, status", "executable": true},
        {"name": "cron",      "description": "Schedule recurring tasks",             "executable": true},
        {"name": "version",   "description": "Show the version",                     "executable": true},
        // No deacon RPC path — UI must route to a terminal or explain.
        {"name": "profile",   "description": "Switch or manage profiles",            "executable": false},
        {"name": "gateway",   "description": "Start/stop the messaging gateway",     "executable": false},
        {"name": "auth",      "description": "Manage gateway authorization",         "executable": false},
        {"name": "logs",      "description": "Tail deacon logs",                     "executable": false},
        {"name": "doctor",    "description": "Diagnose configuration",               "executable": false},
        {"name": "security",  "description": "Review security settings",             "executable": false},
    ])
}
