// The CLI command surface — single source of truth shared by the welcome
// panel's "Commands" section and `regent help`. These are shell subcommands
// (run as `regent <name> …`), distinct from the in-chat slash commands
// (/help, /new, /stop, /approve, /deny) handled inside a chat session.

// Grouped by category (Hermes lists its commands/skills by category). The
// welcome panel renders these as `category: a, b, c` lines.
export const CLI_COMMAND_GROUPS: Record<string, readonly string[]> = {
  session: ["chat", "sessions", "memory", "status"],
  model: ["model", "skills", "tools"],
  config: ["config", "profile", "setup"],
  gateway: ["gateway", "auth"],
  ops: ["cron", "logs", "doctor", "security", "insights", "debug", "mcp", "version"],
};

// Flat list (every command, any group) — used by callers that just need names.
export const CLI_COMMANDS = Object.values(CLI_COMMAND_GROUPS).flat();
