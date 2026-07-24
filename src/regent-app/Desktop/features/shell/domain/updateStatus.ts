// Update-verdict shaping for the status bar (ADR-041, Phase 0 — notify only).
// Pure: no React, no Tauri — just the deacon's additive `update.status` body →
// a typed verdict the badge can render. The release URL is a FIXED official
// constant; a remote/manifest URL is never read or trusted here.

/** The official releases page. A constant, never taken from remote data. */
export const RELEASES_URL = 'https://github.com/Regent33/Regent/releases/latest';

/**
 * The subset of the additive `update.status` body the badge reads. The deacon
 * also sends `checked_at`, `source`, and an optional `note`; unknown or extra
 * fields are ignored (additive-safe — an old peer just omits them).
 */
export interface UpdateStatus {
  /** The deacon's own version (the component that made the comparison). */
  readonly current: string;
  /** Newest published release the deacon found (non-empty when present). */
  readonly latest: string;
}

/**
 * Defensive parse of a raw `update.status` result. Returns null unless an
 * upgrade is actually offered (`available === true` with a known `latest`), so
 * an old deacon, a missing method, or a malformed body simply renders no badge.
 */
export function parseUpdateStatus(raw: unknown): UpdateStatus | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const v = raw as Record<string, unknown>;
  if (v.available !== true) return null;
  if (typeof v.latest !== 'string' || v.latest === '') return null;
  return {
    current: typeof v.current === 'string' ? v.current : '',
    latest: v.latest,
  };
}
