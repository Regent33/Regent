'use client';
// The coding panel: file tree + editor + git actions, collapsed by default.
// Save/commit/push are disabled while the session is busy so a manual edit
// can't race a code task running against the same tree; the write RPC's own
// revision check is the real backstop (a stale buffer is refused, not merged).
import { lazy, Suspense, useEffect } from 'react';
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { isSaveShortcut, languageForPath } from '@/features/workspace/domain/workspaceModel';
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
  readonly ensureSession: (
    workspace?: string,
  ) => Promise<{ ok: true; id: string } | { ok: false; error: string }>;
}

export function WorkspacePanel({ sessionId, busy, ensureSession }: WorkspacePanelProps) {
  const s = t().workspace;
  const { root, isDefault } = useWorkspaceRoot(sessionId);
  const tree = useFileTree(sessionId);
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
    <aside className="flex h-full w-90 shrink-0 flex-col border-l border-stroke-tertiary">
      <header className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1.5">
        <span className="flex-1 truncate text-[11px] text-text-tertiary" title={root}>
          {isDefault ? s.sandboxLabel : (root ?? '')}
        </span>
        {isDefault && (
          <Button size="sm" variant="ghost" title={s.openFolderHint} onClick={pickFolder}>
            {s.openFolder}
          </Button>
        )}
      </header>

      <div className="flex min-h-0 flex-1">
        <div className="w-1/2 min-w-0 overflow-y-auto border-r border-stroke-tertiary py-1">
          {tree.error !== undefined && <ErrorState compact description={tree.error} />}
          <FileTree
            levels={tree.levels}
            expanded={tree.expanded}
            openPath={file.file?.path}
            onToggle={tree.toggle}
            onOpen={(path) => void file.open(path)}
          />
        </div>

        <div className="flex w-1/2 min-w-0 flex-col">
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
