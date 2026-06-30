// The CLI command surface — single source of truth shared by the welcome
// panel's "Commands" section and `regent help`. These are shell subcommands
// (run as `regent <name> …`), distinct from the in-chat slash commands
// (/help, /new, /stop, /approve, /deny) handled inside a chat session.

// Grouped by category. The
// welcome panel renders these as `category: a, b, c` lines.
export const CLI_COMMAND_GROUPS: Record<string, readonly string[]> = {
  session: ["chat", "sessions", "memory", "status"],
  board: ["kanban", "agents"],
  model: ["model", "providers", "skills", "tools"],
  config: ["config", "profile", "setup", "keys", "persona", "soul", "about"],
  gateway: ["gateway", "auth"],
  voice: ["voice", "call"],
  ops: ["cron", "logs", "doctor", "security", "insights", "debug", "mcp", "version"],
};

// In-chat slash commands (typed inside a chat session, not the shell). Shown in
// the welcome panel's command list so the greeting advertises both surfaces.
export const CHAT_SLASH = ["/help", "/doctor", "/new", "/stop", "/approve", "/deny"] as const;

// Flat list (every command, any group) — used by callers that just need names.
export const CLI_COMMANDS = Object.values(CLI_COMMAND_GROUPS).flat();

// --- `/` slash-command picker (Claude-Code-style autocomplete) ---------------

export interface SlashCommand {
  readonly name: string;
  readonly description: string;
}

// Single source of truth for the in-chat `/` menu: the commands that produce
// output in a chat session (terminal-only ones — chat/setup/mcp/debug — are
// omitted) plus the chat-only controls (new/clear/quit). Order = menu order.
export const SLASH_COMMANDS: readonly SlashCommand[] = [
  { name: "help", description: "List commands and usage" },
  { name: "new", description: "Start a fresh conversation" },
  { name: "clear", description: "Clear the conversation" },
  { name: "stop", description: "Interrupt the running turn" },
  { name: "approve", description: "Approve the pending action" },
  { name: "deny", description: "Deny the pending action" },
  { name: "status", description: "Agent + provider status" },
  { name: "sessions", description: "List or resume sessions" },
  { name: "memory", description: "Browse and manage memory" },
  { name: "kanban", description: "View and manage the board" },
  { name: "agents", description: "Manage named persistent agents" },
  { name: "model", description: "Show or set the model" },
  { name: "providers", description: "Manage model providers" },
  { name: "skills", description: "List available skills" },
  { name: "tools", description: "List or toggle tools" },
  { name: "voice", description: "Voice (ASR/TTS): setup, enable, status" },
  { name: "config", description: "Show configuration" },
  { name: "profile", description: "Switch or manage profiles" },
  { name: "keys", description: "Manage provider API keys" },
  { name: "persona", description: "Show persona (soul + about)" },
  { name: "soul", description: "Show or edit the soul" },
  { name: "about", description: "Show or edit the about" },
  { name: "gateway", description: "Start/stop the messaging gateway" },
  { name: "auth", description: "Manage gateway authorization" },
  { name: "cron", description: "Schedule recurring tasks" },
  { name: "logs", description: "Tail daemon logs" },
  { name: "doctor", description: "Diagnose configuration" },
  { name: "security", description: "Review security settings" },
  { name: "insights", description: "Show usage insights" },
  { name: "version", description: "Show the version" },
  { name: "quit", description: "Exit Regent" },
];

// Matches to show in the `/` picker for the current input, or null when it
// shouldn't open: no leading `/`, or a space already typed (the user has moved
// on to arguments). Prefix match on the command name, case-insensitive.
export function matchSlash(input: string): readonly SlashCommand[] | null {
  if (!input.startsWith("/") || input.includes(" ")) return null;
  const q = input.slice(1).toLowerCase();
  return SLASH_COMMANDS.filter((c) => c.name.startsWith(q));
}
