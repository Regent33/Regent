'use client';
// The coding panel's data layer: workspace root, a lazily-expanded file tree,
// the open file's buffer, and git status/actions. Thin glue over deaconRequest
// — all shaping lives in domain/workspaceModel.ts, which is where the tests are.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import {
  type GitStatus,
  type TreeEntry,
  toGitStatus,
  toTreeEntries,
} from '@/features/workspace/domain/workspaceModel';

export interface OpenFile {
  readonly path: string;
  readonly text: string;
  /** Revision token from the read; `workspace.write` must echo it back. */
  readonly rev: string;
}

export function useWorkspaceRoot(sessionId: string | undefined) {
  const [root, setRoot] = useState<string>();
  const [isDefault, setIsDefault] = useState(true);

  useEffect(() => {
    if (sessionId === undefined) {
      setRoot(undefined);
      return;
    }
    let alive = true;
    void deaconRequest('workspace.get', { session_id: sessionId }).then((result) => {
      if (!alive || !result.ok) return;
      const value = result.value as { root?: string; is_default?: boolean };
      setRoot(value?.root);
      setIsDefault(value?.is_default !== false);
    });
    return () => {
      alive = false;
    };
  }, [sessionId]);

  return { root, isDefault };
}

/** Per-directory tree cache. Levels are fetched on first expand and kept, so
 * collapsing and re-expanding doesn't refetch. */
/** `only` scopes the ROOT level to these folder names (a session's own work in
 * the shared sandbox). Empty = show everything; deeper levels are never
 * filtered, since once you're inside a folder it's all yours. */
export function useFileTree(sessionId: string | undefined, only?: ReadonlySet<string>) {
  const [levels, setLevels] = useState<Record<string, readonly TreeEntry[]>>({});
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [error, setError] = useState<string>();

  const load = useCallback(
    async (path: string) => {
      if (sessionId === undefined) return;
      const result = await deaconRequest('workspace.tree', { session_id: sessionId, path });
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      setError(undefined);
      const entries = toTreeEntries(result.value);
      const scoped =
        path === '' && only !== undefined && only.size > 0
          ? entries.filter((e) => only.has(e.name))
          : entries;
      setLevels((prev) => ({ ...prev, [path]: scoped }));
    },
    [sessionId, only],
  );

  // Root level whenever the session changes; deeper levels load on expand.
  useEffect(() => {
    setLevels({});
    setExpanded(new Set());
    void load('');
  }, [load]);

  const toggle = useCallback(
    (path: string) => {
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(path)) {
          next.delete(path);
        } else {
          next.add(path);
          void load(path);
        }
        return next;
      });
    },
    [load],
  );

  return { levels, expanded, error, toggle, reload: load };
}

export function useOpenFile(sessionId: string | undefined) {
  const [file, setFile] = useState<OpenFile>();
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  const open = useCallback(
    async (path: string) => {
      if (sessionId === undefined) return;
      const result = await deaconRequest('workspace.read', { session_id: sessionId, path });
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      const value = result.value as { text?: string; rev?: string; binary?: boolean };
      if (value?.binary === true || typeof value?.text !== 'string') {
        setFile(undefined);
        setError('This file is binary and can’t be edited here.');
        return;
      }
      setError(undefined);
      setFile({ path, text: value.text, rev: value.rev ?? '' });
      setDraft(value.text);
    },
    [sessionId],
  );

  const save = useCallback(async (): Promise<boolean> => {
    if (sessionId === undefined || file === undefined) return false;
    setSaving(true);
    const result = await deaconRequest('workspace.write', {
      session_id: sessionId,
      path: file.path,
      content: draft,
      rev: file.rev,
    });
    setSaving(false);
    if (!result.ok) {
      // Includes the stale-rev refusal: the file moved under the editor (a
      // code task edited it), so the user is told rather than silently
      // clobbering the newer content.
      setError(result.error.message);
      return false;
    }
    const value = result.value as { rev?: string };
    setFile({ path: file.path, text: draft, rev: value?.rev ?? '' });
    setError(undefined);
    return true;
  }, [sessionId, file, draft]);

  const dirty = file !== undefined && draft !== file.text;
  return { file, draft, setDraft, open, save, dirty, saving, error, clearError: () => setError(undefined) };
}

export function useGit(sessionId: string | undefined) {
  const [status, setStatus] = useState<GitStatus>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const refresh = useCallback(async () => {
    if (sessionId === undefined) return;
    const result = await deaconRequest('git.status', { session_id: sessionId });
    if (!result.ok) {
      setError(result.error.message);
      return;
    }
    setStatus(toGitStatus(result.value));
  }, [sessionId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = useCallback(
    async (method: string, params: Record<string, unknown>): Promise<boolean> => {
      if (sessionId === undefined) return false;
      setBusy(true);
      const result = await deaconRequest(method, { session_id: sessionId, ...params });
      setBusy(false);
      if (!result.ok) {
        // Surfaced verbatim: git's own "no upstream" / auth / conflict text
        // names the command that fixes it.
        setError(result.error.message);
        return false;
      }
      setError(undefined);
      await refresh();
      return true;
    },
    [sessionId, refresh],
  );

  return {
    status,
    busy,
    error,
    refresh,
    clearError: () => setError(undefined),
    commit: (message: string) => run('git.commit', { message }),
    push: () => run('git.push', {}),
  };
}
