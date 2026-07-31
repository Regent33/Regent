// The CLI's exit-code taxonomy, defined once. Scattered `process.exit(2)` calls
// drift; a shared constant does not. Codes beyond these three are reserved for
// the headless surface and get added when it ships, not before.
export const EXIT = {
  ok: 0,
  /** The command ran and failed. */
  failure: 1,
  /** The invocation was wrong: unknown command or option, bad usage. */
  usage: 2,
} as const;
