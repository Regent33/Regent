// Structured logger — writes one JSON line per event to STDERR only, never
// stdout (stdout belongs to Ink's render; the deacon's JSON-RPC is on its own
// child pipes). Section 9: log at service boundaries; never log secrets/PII.

type Outcome = "success" | "failure";

interface LogFields {
  readonly operation: string;
  readonly outcome?: Outcome;
  readonly [key: string]: unknown;
}

// `info` is the chatty level (e.g. the bootstrap line) — it interleaves with
// Ink's render and clutters the interactive CLI, so it's opt-in via REGENT_LOG.
// `warn`/`error` always surface.
const VERBOSE = process.env.REGENT_LOG === "info" || process.env.REGENT_LOG === "debug";

function emit(level: "info" | "warn" | "error", fields: LogFields): void {
  if (level === "info" && !VERBOSE) return;
  const record = { ts: new Date().toISOString(), level, ...fields };
  process.stderr.write(`${JSON.stringify(record)}\n`);
}

export const logger = {
  info: (fields: LogFields) => emit("info", fields),
  warn: (fields: LogFields) => emit("warn", fields),
  error: (fields: LogFields) => emit("error", fields),
};
