'use client';
// The coding panel's data layer: workspace root, a lazily-expanded file tree,
// the open file's buffer, and git status/actions. Thin glue over deaconRequest
// — all shaping lives in domain/workspaceModel.ts, which is where the tests are.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import {
  type GitStatus,
  type TreeEntry,
  remainingHold,
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
  // True while the ROOT listing is in flight. A real repo takes a moment to
  // list, and with no flag the panel rendered an empty tree and then popped the
  // whole thing in at once — indistinguishable from an empty folder while it
  // waited. Deeper levels don't need this: their row is already on screen.
  const [loadingRoot, setLoadingRoot] = useState(true);

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
    setLoadingRoot(true);
    // Same hold as the file open: a small repo lists instantly and the skeleton
    // was gone before anyone saw it.
    const startedAt = Date.now();
    void load('')
      .then(() => holdIndicator(startedAt))
      .finally(() => setLoadingRoot(false));
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

  /** New file/folder, then re-list the directory it landed in so it shows up
   * without a manual refresh. `path` is workspace-relative. */
  const create = useCallback(
    async (path: string, kind: 'file' | 'dir'): Promise<boolean> => {
      if (sessionId === undefined) return false;
      const result = await deaconRequest('workspace.create', {
        session_id: sessionId,
        path,
        kind,
      });
      if (!result.ok) {
        setError(result.error.message);
        return false;
      }
      setError(undefined);
      const slash = path.lastIndexOf('/');
      await load(slash === -1 ? '' : path.slice(0, slash));
      return true;
    },
    [sessionId, load],
  );

  /** Re-fetch every level currently on screen — the explorer's Refresh. */
  const refresh = useCallback(async () => {
    await Promise.all(Object.keys(levels).map((dir) => load(dir)));
  }, [levels, load]);

  return { levels, expanded, error, loadingRoot, toggle, reload: load, create, refresh };
}

/** Keeps a loading indicator on screen long enough to register.
 *
 * Fire-and-await after the work finishes: `remainingHold` returns 0 once the work
 * already took longer than the minimum, so a slow load is never delayed further.
 */
async function holdIndicator(startedAt: number): Promise<void> {
  const wait = remainingHold(Date.now() - startedAt);
  if (wait === 0) return;
  await new Promise((resolve) => setTimeout(resolve, wait));
}

export function useOpenFile(sessionId: string | undefined) {
  const [file, setFile] = useState<OpenFile>();
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);
  // In flight for `open`. Without it the editor pane kept showing "Select a
  // file to edit." for the whole read, so clicking a file in a big repo looked
  // like the click had missed. Carries the path, not just a boolean, so the
  // placeholder can name what it is loading.
  const [opening, setOpening] = useState<string>();

  const open = useCallback(
    async (path: string) => {
      if (sessionId === undefined) return;
      setOpening(path);
      const startedAt = Date.now();
      const result = await deaconRequest('workspace.read', { session_id: sessionId, path });
      // Held so the indicator is actually seen: a local read finishes in single
      // digit milliseconds, which made the whole thing invisible. See
      // MIN_LOADING_MS for why this delay is deliberate.
      await holdIndicator(startedAt);
      setOpening(undefined);
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

  /** Pick up an external edit (the agent, or another editor) WITHOUT
   * clobbering unsaved work: a dirty buffer is left alone, and the save path's
   * revision check still refuses to overwrite the newer file. */
  const reloadIfClean = useCallback(async () => {
    if (sessionId === undefined || file === undefined || dirty) return;
    const result = await deaconRequest('workspace.read', {
      session_id: sessionId,
      path: file.path,
    });
    if (!result.ok) return;
    const value = result.value as { text?: string; rev?: string };
    if (typeof value?.text !== 'string' || value.rev === file.rev) return;
    setFile({ path: file.path, text: value.text, rev: value.rev ?? '' });
    setDraft(value.text);
  }, [sessionId, file, dirty]);

  return {
    file,
    draft,
    setDraft,
    open,
    save,
    dirty,
    saving,
    opening,
    error,
    reloadIfClean,
    clearError: () => setError(undefined),
  };
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
