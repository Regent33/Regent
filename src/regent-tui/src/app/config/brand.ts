// Brand + all user-facing copy in one place (i18n-ready: components never
// hardcode strings). Persona per ADR-012: kind, thoughtful, warm, light emoji.

export const BRAND = {
  name: "Regent .✦ ",
  version: "0.1.0",
  tagline: "a personal AI agent",
} as const;

export const COPY = {
  connecting: "Summoning Regent…",
  welcome: " ✧ Welcome! I'm Regent — at your service. 🤍",
  exitHint: "press q or Ctrl-C to exit",
  sessionHeading: "Session",
  commandsHeading: "Commands",
  skillsHeading: "Skills",
  modelLabel: "model",
  sessionLabel: "session",
  noCommands: "—",
  skillsSummary: (n: number) => `${n} learned — they grow as we work together`,
  errorTitle: "Couldn't reach the daemon",
  errorHint: "Build it with `cargo build -p regent-daemon`, or run `regent doctor`.",

  // Chat surface
  inputPlaceholder: "Type a message…",
  approvePrompt: "Allow it? [y/N]",
  awaitingApproval: "awaiting your approval",
  thinking: "thinking… (Ctrl-C to interrupt)",
  idleHint: "/quit to exit · Enter to send",
  approved: "✓ approved",
  denied: "✗ denied",
  toolRunning: (tool: string) => `⚙ ${tool}…`,
  toolSnag: (tool: string) => `✗ ${tool} hit a snag`,
  approvalWarn: (tool: string) => `⚠ ${tool} wants to run a sensitive action:`,
  delivered: (target: string) => `✉ delivered to ${target}`,
  submitError: (message: string) => `⚠ ${message}`,
} as const;
