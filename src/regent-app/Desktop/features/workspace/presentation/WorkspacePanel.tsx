'use client';
// The coding panel: file tree + editor + git actions, collapsed by default.
// Save/commit/push are disabled while the session is busy so a manual edit
// can't race a code task running against the same tree; the write RPC's own
// revision check is the real backstop (a stale buffer is refused, not merged).
import { lazy, Suspense, useEffect, useMemo } from 'react';
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { CloseIcon, CollapseIcon, ExpandIcon } from '@/shared/ui/icons';
import { useDragSize } from '@/features/workspace/viewmodels/useDragSize';
import {
  isSaveShortcut,
  languageForPath,
  sessionFolders,
} from '@/features/workspace/domain/workspaceModel';
import { FileTree } from '@/features/workspace/presentation/FileTree';
import { GitToolbar } from '@/features/workspace/presentation/GitToolbar';
import {
  useFileTree,
  useGit,
  useOpenFile,
  useWorkspaceRoot,
} from '@/features/workspace/viewmodels/useWorkspace';

// Monaco is multiple MB. Loaded only when the panel is open AND a file is
// picked, mirroring how ButlerView is code-split — it never touches the
// initial chat bundle.
const CodeEditor = lazy(() =>
  import('@/features/workspace/presentation/CodeEditor').then((m) => ({ default: m.CodeEditor })),
);

interface WorkspacePanelProps {
  readonly sessionId: string | undefined;
  readonly busy: boolean;
  /** File paths this session's tools touched — the tree scopes the shared
   * sandbox down to the folders this conversation actually produced. */
  readonly touchedPaths: readonly string[];
  readonly ensureSession: (
    workspace?: string,
  ) => Promise<{ ok: true; id: string } | { ok: false; error: string }>;
  /** Owned by ChatView: maximizing HIDES the chat column, which only the
   * parent can do, so the flag and its toggle live up there. */
  readonly maximized: boolean;
  readonly onToggleMaximize: () => void;
  readonly onClose: () => void;
}

export function WorkspacePanel({
  sessionId,
  busy,
  touchedPaths,
  ensureSession,
  maximized,
  onToggleMaximize,
  onClose,
}: WorkspacePanelProps) {
  const s = t().workspace;
  // Never let the drag squeeze the chat into an unreadable ribbon — the cap is
  // "window minus a usable chat column", recomputed per drag.
  const maxPanel = typeof window === 'undefined' ? 900 : Math.max(320, window.innerWidth - 460);
  const panel = useDragSize(360, 260, maxPanel, -1);
  const split = useDragSize(180, 120, 640, 1);
  const { root, isDefault } = useWorkspaceRoot(sessionId);
  // Only the shared sandbox needs scoping — a folder the user opened is
  // already theirs, so it lists in full.
  const only = useMemo(
    () => (isDefault && root !== undefined ? sessionFolders(touchedPaths, root) : undefined),
    [isDefault, root, touchedPaths],
  );
  const tree = useFileTree(sessionId, only);
  const file = useOpenFile(sessionId);
  const git = useGit(sessionId);

  // Ctrl/Cmd+S saves the open file. Capture phase so it beats the webview's
  // own save-page handling; only armed when a save is actually possible.
  const canSave = file.dirty && !busy && !file.saving;
  useEffect(() => {
    if (!canSave) return;
    const onKey = (e: KeyboardEvent) => {
      if (!isSaveShortcut(e)) return;
      e.preventDefault();
      void file.save().then((ok) => {
        if (ok) void git.refresh();
      });
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => window.removeEventListener('keydown', onKey, { capture: true });
  }, [canSave, file, git]);

  // A folder can be opened before any message is sent, so this may be what
  // creates the session; ensureSession single-flights against the composer.
  const pickFolder = async () => {
    const picked = await openFolderDialog({ directory: true, multiple: false });
    if (typeof picked !== 'string') return;
    await ensureSession(picked);
  };

  return (
    <aside
      className={`relative flex h-full flex-col border-l border-stroke-tertiary ${
        maximized ? 'min-w-0 flex-1' : 'shrink-0'
      }`}
      style={maximized ? undefined : { width: `${panel.size}px` }}
    >
      {/* Left-edge drag handle. 4px wide but sits in its own column so it
          never overlaps the tree's scrollbar. */}
      {!maximized && (
        <div
          role="separator"
          aria-orientation="vertical"
          className="absolute inset-y-0 -left-0.5 z-10 w-1 cursor-col-resize hover:bg-accent/40"
          {...panel.handleProps}
        />
      )}
      <header className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1.5">
        <span className="flex-1 truncate text-[11px] text-text-tertiary" title={root}>
          {isDefault ? s.sandboxLabel : (root ?? '')}
        </span>
        {isDefault && (
          <Button size="sm" variant="ghost" title={s.openFolderHint} onClick={pickFolder}>
            {s.openFolder}
          </Button>
        )}
        <Button
          size="iconSm"
          variant="ghost"
          aria-label={maximized ? s.restore : s.maximize}
          title={maximized ? s.restore : s.maximize}
          onClick={onToggleMaximize}
        >
          {maximized ? <CollapseIcon /> : <ExpandIcon />}
        </Button>
        {/* The floating "Files" toggle is hidden while the panel is open (it
            collided with this edge), so closing has to live in here. */}
        <Button size="iconSm" variant="ghost" aria-label={s.close} title={s.close} onClick={onClose}>
          <CloseIcon />
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        <div
          className="min-w-0 shrink-0 overflow-y-auto py-1"
          style={{ width: `${split.size}px` }}
        >
          {tree.error !== undefined && <ErrorState compact description={tree.error} />}
          <FileTree
            levels={tree.levels}
            expanded={tree.expanded}
            openPath={file.file?.path}
            onToggle={tree.toggle}
            onOpen={(path) => void file.open(path)}
          />
        </div>

        {/* Tree/editor divider — same drag mechanism, horizontal direction. */}
        <div
          role="separator"
          aria-orientation="vertical"
          className="w-1 shrink-0 cursor-col-resize border-r border-stroke-tertiary hover:bg-accent/40"
          {...split.handleProps}
        />

        <div className="flex min-w-0 flex-1 flex-col">
          {file.error !== undefined && (
            <div className="p-2">
              <ErrorState compact description={file.error} />
              <button
                type="button"
                className="mt-1 text-[11px] text-text-tertiary"
                onClick={file.clearError}
              >
                {s.dismiss}
              </button>
            </div>
          )}
          {file.file === undefined ? (
            <p className="p-3 text-[12px] text-text-tertiary">{s.noFileOpen}</p>
          ) : (
            <>
              <div className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1">
                <span className="flex-1 truncate text-[11px] text-text-tertiary">
                  {file.file.path}
                  {file.dirty && <span className="ml-1 text-accent">•</span>}
                </span>
                <Button
                  size="sm"
                  disabled={!canSave}
                  title={busy ? s.busyHint : undefined}
                  onClick={() =>
                    void file.save().then((ok) => {
                      if (ok) void git.refresh();
                    })
                  }
                >
                  {file.saving ? s.saving : s.save}
                </Button>
              </div>
              <div className="min-h-0 flex-1">
                <Suspense fallback={<Loader />}>
                  <CodeEditor
                    value={file.draft}
                    language={languageForPath(file.file.path)}
                    readOnly={busy}
                    onChange={file.setDraft}
                  />
                </Suspense>
              </div>
            </>
          )}
        </div>
      </div>

      <GitToolbar
        status={git.status}
        busy={git.busy || busy}
        error={git.error}
        onClearError={git.clearError}
        onCommit={git.commit}
        onPush={git.push}
      />
    </aside>
  );
}
