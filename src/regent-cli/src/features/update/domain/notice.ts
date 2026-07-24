// Update-notice shaping (ADR-041, Phase 0 — notify only). Pure: no I/O, no RPC.
// Turns the deacon's additive `update.status` verdict into one actionable line,
// and only when an upgrade is actually offered. The download link is a FIXED
// official URL — a remote/manifest URL is never read or trusted here.

/** The official releases page. A constant, never taken from remote data. */
export const RELEASES_URL = "https://github.com/Regent33/Regent/releases/latest";

/**
 * The subset of the additive `update.status` body this surface reads. The
 * deacon also sends `checked_at`, `source`, and an optional `note`; unknown or
 * extra fields are ignored (additive-safe — an old peer just omits them).
 */
export interface UpdateStatus {
  /** The deacon's own version (the component that performed the comparison). */
  readonly current: string;
  /** Newest published release, or null when unknown / never checked. */
  readonly latest: string | null;
  /** The deacon judged `latest` strictly newer than its `current`. */
  readonly available: boolean;
}

/** Defensive parse of a raw `update.status` result. Returns null unless the
 *  minimal Phase-0 shape is present, so a malformed or partial body degrades to
 *  "nothing to say" rather than throwing. */
export function parseUpdateStatus(raw: unknown): UpdateStatus | null {
  if (typeof raw !== "object" || raw === null) return null;
  const v = raw as Record<string, unknown>;
  if (typeof v.available !== "boolean") return null;
  const latest = typeof v.latest === "string" && v.latest !== "" ? v.latest : null;
  return {
    current: typeof v.current === "string" ? v.current : "",
    latest,
    available: v.available,
  };
}

/**
 * One concise, actionable line — or null when there is nothing to say (no
 * status, no upgrade offered, or no known `latest`). The wording names this
 * CLI's OWN version and does not claim the deacon and the CLI are the same
 * component; it only reports the newest release the deacon found.
 */
export function updateNotice(status: UpdateStatus | null, cliVersion: string): string | null {
  if (status === null || !status.available || status.latest === null) return null;
  return `Regent ${status.latest} is available (this CLI is ${cliVersion}) — ${RELEASES_URL}`;
}
