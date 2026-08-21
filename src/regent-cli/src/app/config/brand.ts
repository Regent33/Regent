// Brand + all user-facing copy in one place (i18n-ready: components never
// hardcode strings). Persona per ADR-012: kind, thoughtful, warm, light emoji.

export const BRAND = {
  name: "Regent .✦ ",
  version: "0.1.4",
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
  errorTitle: "Couldn't reach the deacon",
  errorHint: "Build it with `cargo build -p regent-deacon`, or run `regent doctor`.",

  // Chat surface
  inputPlaceholder: "Type a message…",
  queuePlaceholder: "type a follow-up — it'll send when this turn finishes…",
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
  // Only `finished` is good news; relaying a timed-out or cancelled job as done
  // is the laundering the job note's own text forbids.
  jobFinished: (label: string, state: string) =>
    `${state === "finished" ? "✅" : "⚠️"} ${label} — ${state}`,
  // Structured questions (the ask_user card)
  awaitingAnswer: "awaiting your answer",
  questionAsked: (n: number) =>
    n === 1 ? "⁇ Regent has a question" : `⁇ Regent has ${n} questions`,
  questionStep: (at: number, of: number) => (of === 1 ? "" : ` (${at} of ${of})`),
  questionCustomRow: "Something else…",
  questionCustomPrompt: "Type your answer…",
  questionCancelled: "✗ question dismissed",
  questionAnswer: (prompt: string, answer: string) => `  ${prompt} → ${answer}`,
  questionKeysSingle: "↑↓ move · 1-9 jump · ↵ choose · esc skip",
  questionKeysMulti: "↑↓ move · space toggle · 1-9 jump · ↵ submit · esc skip",
  questionPlaceholder: "answer below, or use the list above…",
  submitError: (message: string) =>
    /401|authenticat|unauthor/i.test(message)
      ? `⚠ ${message}\n  → your API key was rejected. Run \`regent setup\` to set a valid key (or export REGENT_API_KEY).`
      : `⚠ ${message}`,
} as const;
