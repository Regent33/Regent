'use client';
// Artifacts master list — artifacts.list {} returns one entry per slug dir
// under $REGENT_HOME/artifacts, newest first: {name, created_at, files:
// [{name, rel, bytes, kind}]} (artifacts_ops.rs::list_artifacts). Flattened
// here into per-file rows for the master list; grouping by slug is kept via
// `slug` so the presentation layer can render one section per run. A missing
// or empty root comes back as `[]`, never an error.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export type ArtifactKind = 'text' | 'image' | 'other';

export interface ArtifactFileRow {
  readonly slug: string;
  readonly name: string;
  readonly rel: string;
  readonly bytes: number;
  readonly kind: ArtifactKind;
}

export interface ArtifactGroup {
  readonly slug: string;
  readonly createdAt?: number;
  readonly files: readonly ArtifactFileRow[];
}

export interface ArtifactsListState {
  readonly groups: readonly ArtifactGroup[];
  readonly loading: boolean;
  readonly error?: string;
}

function toKind(value: unknown): ArtifactKind {
  return value === 'text' || value === 'image' ? value : 'other';
}

function toFileRow(slug: string, value: unknown): ArtifactFileRow | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const name = typeof v.name === 'string' ? v.name : undefined;
  const rel = typeof v.rel === 'string' ? v.rel : undefined;
  if (name === undefined || rel === undefined) return undefined;
  return { slug, name, rel, bytes: typeof v.bytes === 'number' ? v.bytes : 0, kind: toKind(v.kind) };
}

function toGroup(value: unknown): ArtifactGroup | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const name = typeof v.name === 'string' ? v.name : undefined;
  if (name === undefined) return undefined;
  const files = Array.isArray(v.files) ? v.files : [];
  return {
    slug: name,
    createdAt: typeof v.created_at === 'number' ? v.created_at : undefined,
    files: files.map((f) => toFileRow(name, f)).filter((f): f is ArtifactFileRow => f !== undefined),
  };
}

export function useArtifactsList(): ArtifactsListState {
  const [groups, setGroups] = useState<readonly ArtifactGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    void deaconRequest('artifacts.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setGroups(list.map(toGroup).filter((g): g is ArtifactGroup => g !== undefined));
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  return { groups, loading, error };
}
