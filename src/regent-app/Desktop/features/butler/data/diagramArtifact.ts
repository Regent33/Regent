import { extractPresentSpec, type PresentSpec } from '@/shared/diagram/presentSpec';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';

type ArtifactRequestResult =
  | { ok: true; value: unknown }
  | { ok: false; error?: { message?: string } };

export type ArtifactRequester = (
  method: string,
  params: Record<string, unknown>,
) => Promise<ArtifactRequestResult>;

/** Finds a safe artifact-relative `.../diagram.json` reference in model prose.
 * Absolute `$REGENT_HOME/artifacts/...` paths are reduced to the relative path
 * accepted by `artifacts.get`; unrelated JSON paths are ignored. */
export function extractDiagramArtifactPath(reply: string): string | null {
  const normalized = reply.replace(/\\/g, '/');
  const match = /(?:^|\/)artifacts\/((?:[^/\n`"']+\/)*diagram\.json)\b/i.exec(normalized);
  if (!match) return null;
  return match[1]
    .split('/')
    .map((segment) => segment.trim())
    .filter(Boolean)
    .join('/');
}

/** Recovers the common weak-model failure where a valid PresentSpec was saved
 * to an artifact instead of emitted inline. The backend remains the path
 * traversal boundary; this function only sends a relative artifact path. */
export async function recoverDiagramArtifact(
  reply: string,
  request: ArtifactRequester = deaconRequest as ArtifactRequester,
): Promise<PresentSpec | null> {
  const path = extractDiagramArtifactPath(reply);
  if (!path) return null;
  const result = await request('artifacts.get', { path }).catch(() => null);
  if (!result?.ok || typeof result.value !== 'object' || result.value === null) return null;
  const text = (result.value as Record<string, unknown>).text;
  if (typeof text !== 'string') return null;
  return extractPresentSpec(text).spec;
}
