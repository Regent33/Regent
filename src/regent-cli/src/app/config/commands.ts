// The CLI command surface — single source of truth shared by the welcome
// panel's "Commands" section and `regent help`. These are shell subcommands
// (run as `regent <name> …`), distinct from the in-chat slash commands
// (/help, /new, /stop, /approve, /deny) handled inside a chat session.

// Grouped by category (Hermes lists its commands/skills by category). The
// welcome panel renders these as `category: a, b, c` lines.
export const CLI_COMMAND_GROUPS: Record<string, readonly string[]> = {
  session: ["chat", "sessions", "memory", "status"],
  board: ["kanban"],
  model: ["model", "skills", "tools"],
  config: ["config", "profile", "setup", "persona", "soul", "about"],
  gateway: ["gateway", "auth"],
  ops: ["cron", "logs", "doctor", "security", "insights", "debug", "mcp", "version"],
};

// In-chat slash commands (typed inside a chat session, not the shell). Shown in
// the welcome panel's command list so the greeting advertises both surfaces.
export const CHAT_SLASH = ["/help", "/doctor", "/new", "/stop", "/approve", "/deny"] as const;

// Flat list (every command, any group) — used by callers that just need names.
export const CLI_COMMANDS = Object.values(CLI_COMMAND_GROUPS).flat();
