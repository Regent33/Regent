'use client';
// Artifact detail — artifacts.get {path} resolves a `rel` from the list into
// {path, abs, mime, kind, text?, data_base64?} (artifacts_ops.rs::get_artifact).
// Text/images are inlined only under the backend's size caps (256 KB text,
// 5 MB image); anything larger, or any other kind, comes back with `abs`
// only — the caller falls back to metadata + copy-path. Traversal-unsafe or
// missing paths arrive as a JSON-RPC error, surfaced verbatim.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import type { ArtifactKind } from '@/features/artifacts/viewmodels/useArtifactsList';

export interface ArtifactDetail {
  readonly path: string;
  readonly abs: string;
  readonly mime: string;
  readonly kind: ArtifactKind;
  readonly text?: string;
  readonly dataBase64?: string;
}

export interface ArtifactDetailState {
  readonly detail?: ArtifactDetail;
  readonly loading: boolean;
  readonly error?: string;
}

function toDetail(value: unknown): ArtifactDetail | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const path = typeof v.path === 'string' ? v.path : undefined;
  const abs = typeof v.abs === 'string' ? v.abs : undefined;
  const mime = typeof v.mime === 'string' ? v.mime : undefined;
  if (path === undefined || abs === undefined || mime === undefined) return undefined;
  return {
    path,
    abs,
    mime,
    kind: v.kind === 'text' || v.kind === 'image' ? v.kind : 'other',
    text: typeof v.text === 'string' ? v.text : undefined,
    dataBase64: typeof v.data_base64 === 'string' ? v.data_base64 : undefined,
  };
}

export function useArtifactDetail(rel: string | undefined): ArtifactDetailState {
  const [detail, setDetail] = useState<ArtifactDetail>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (rel === undefined) {
      setDetail(undefined);
      setError(undefined);
      return;
    }
    if (!isTauri()) return;
    let alive = true;
    setLoading(true);
    setError(undefined);
    void deaconRequest('artifacts.get', { path: rel }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setDetail(undefined);
        setLoading(false);
        return;
      }
      setDetail(toDetail(result.value));
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [rel]);

  return { detail, loading, error };
}
