'use client';
// The coding panel: file tree + editor + git actions, collapsed by default.
// Save/commit/push are disabled while the session is busy so a manual edit
// can't race a code task running against the same tree; the write RPC's own
// revision check is the real backstop (a stale buffer is refused, not merged).
import { lazy, Suspense, useEffect, useMemo, useState } from 'react';
import { open as openFolderDialog } from '@tauri-apps/plugin-dialog';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { TreeSkeleton } from '@/features/workspace/presentation/TreeSkeleton';
import {
  CloseIcon,
  CollapseIcon,
  ExpandIcon,
  NewFileIcon,
  NewFolderIcon,
  RefreshIcon,
} from '@/shared/ui/icons';
import { Markdown } from '@/shared/ui/Markdown';
import {
  clearEditorContext,
  setOpenFile,
  setOpenSelection,
  setSelectedFolder,
} from '@/shared/state/openFile';
import { useDragSize } from '@/features/workspace/viewmodels/useDragSize';
import {
  isSaveShortcut,
  languageForPath,
  sessionFolders,
} from '@/features/workspace/domain/workspaceModel';
import { FileTree } from '@/features/workspace/presentation/FileTree';
import { ChangesView } from '@/features/workspace/presentation/ChangesView';
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
  /** Open a folder INTO this session (`workspace.set` rebinds a live one).
   * Resolves to a message when it failed, `undefined` when it worked — a
   * discarded failure is what made picking a folder look like a no-op. */
  readonly onOpenFolder: (path: string) => Promise<string | undefined>;
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
  onOpenFolder,
  maximized,
  onToggleMaximize,
  onClose,
}: WorkspacePanelProps) {
  const s = t().workspace;
  // Never let the drag squeeze the chat into an unreadable ribbon. 560px is
  // what the chat column actually needs to stay usable — the composer's own
  // controls plus readable message width — not just non-zero. Maximize is the
  // way to give the panel the whole window; dragging keeps chat alive.
  const CHAT_MIN_WIDTH = 560;
  const maxPanel =
    typeof window === 'undefined' ? 900 : Math.max(320, window.innerWidth - CHAT_MIN_WIDTH);
  const panel = useDragSize(360, 260, maxPanel, -1);
  // The tree can't eat the editor either.
  const split = useDragSize(180, 120, 520, 1);
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
  const [creating, setCreating] = useState<{ kind: 'file' | 'dir'; name: string }>();
  // Markdown opens in the editor; this flips it to a rendered preview.
  const [preview, setPreview] = useState(false);
  const [showChanges, setShowChanges] = useState(false);
  // What the user last clicked in the tree — a folder counts. This is the
  // create target, so "new file" lands where they are pointing.
  const [selected, setSelected] = useState<{ path: string; isDir: boolean }>();
  // Why a folder pick failed. Cleared on the next attempt, since pickFolder
  // assigns the fresh result unconditionally.
  const [openError, setOpenError] = useState<string>();
  const isMarkdown = file.file !== undefined && languageForPath(file.file.path) === 'markdown';

  /** Where a new file/folder lands, in the order a person would expect:
   * the folder they highlighted, else the folder of the file they highlighted,
   * else the folder of the file being edited, else the root. Previously only
   * the open file counted, so selecting a folder and hitting "new file" put it
   * somewhere else entirely. */
  const createParent = (): string => {
    if (selected?.isDir === true) return selected.path;
    const path = selected?.path ?? file.file?.path;
    if (path === undefined) return '';
    const slash = path.lastIndexOf('/');
    return slash === -1 ? '' : path.slice(0, slash);
  };

  // Keep the panel honest about disk. Files change underneath it constantly —
  // the agent edits during a turn, and the user may edit in another editor —
  // so the tree, git status, and a CLEAN open buffer re-sync on a timer.
  //
  // ponytail: polling, not a filesystem watcher. A watcher means a new Rust
  // dependency, a per-session watch lifecycle, debouncing, and an event
  // channel; a 3s poll of the levels already on screen is a few requests and
  // is indistinguishable at human speed. Swap to notify(5) if the request
  // volume ever shows up.
  useEffect(() => {
    const tick = () => {
      // Skip while hidden: a background window doesn't need fresh listings,
      // and this would otherwise poll forever behind another app.
      if (typeof document !== 'undefined' && document.hidden) return;
      void tree.refresh();
      void git.refresh();
      void file.reloadIfClean();
    };
    const timer = setInterval(tick, 3000);
    return () => clearInterval(timer);
  }, [tree.refresh, git.refresh, file.reloadIfClean]);

  // Publish what the user is looking at, so the next chat turn can tell the
  // agent. Cleared when the panel closes — a file nobody can see isn't context.
  const openPath = file.file?.path;
  useEffect(() => {
    setOpenFile(openPath);
    return () => setOpenFile(undefined);
  }, [openPath]);

  // Panel gone → nothing on screen is context any more, so drop the folder
  // selection too. Unmount-only (empty deps) deliberately: the effect above
  // re-runs per open file, and clearing there would wipe a folder selection
  // every time the user opened a different document.
  useEffect(() => () => clearEditorContext(), []);

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

  const pickFolder = async () => {
    const picked = await openFolderDialog({ directory: true, multiple: false });
    if (typeof picked !== 'string') return;   // dialog dismissed — not a failure
    setOpenError(await onOpenFolder(picked));
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
        {/* Windows canonicalization returns the extended-length form
            (\\?\D:\proj); strip it so the header reads like a path a person
            would type. */}
        <span className="flex-1 truncate text-[11px] text-text-tertiary" title={root}>
          {isDefault ? s.sandboxLabel : (root?.replace(/^\\\\\?\\/, '') ?? '')}
        </span>
        {isDefault && (
          <Button
            size="sm"
            variant="ghost"
            title={sessionId === undefined ? s.openFolderHint : s.openFolderNewChatHint}
            onClick={pickFolder}
          >
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
          {/* Explorer toolbar, VSCode's three: new file, new folder, refresh.
              Creating targets the open folder when there is one, so a file
              lands where the user is looking rather than at the root. */}
          <div className="mb-1 flex items-center gap-0.5 px-1.5">
            <span className="flex-1 truncate text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
              {s.explorer}
            </span>
            <Button size="iconSm" variant="ghost" aria-label={s.newFile} title={s.newFile}
              onClick={() => setCreating({ kind: 'file', name: '' })}>
              <NewFileIcon className="size-3.5" />
            </Button>
            <Button size="iconSm" variant="ghost" aria-label={s.newFolder} title={s.newFolder}
              onClick={() => setCreating({ kind: 'dir', name: '' })}>
              <NewFolderIcon className="size-3.5" />
            </Button>
            <Button size="iconSm" variant="ghost" aria-label={s.refresh} title={s.refresh}
              onClick={() => void tree.refresh()}>
              <RefreshIcon className="size-3.5" />
            </Button>
          </div>
          {/* The row sits under the toolbar rather than inside the tree, so on
              its own it looks like it creates at the ROOT however deep the
              selected folder is. Naming the target is what makes it honest —
              the path is a live echo of createParent(). */}
          {creating !== undefined && (
            <div className="mx-1.5 mb-1 flex items-center gap-1 rounded-sm bg-hover px-1.5 py-1">
              <span
                title={createParent() === '' ? '/' : `${createParent()}/`}
                className="max-w-[45%] shrink-0 truncate text-[11px] text-text-tertiary"
              >
                {createParent() === '' ? '/' : `${createParent().split('/').at(-1)}/`}
              </span>
              <input
                autoFocus
                value={creating.name}
                placeholder={creating.kind === 'dir' ? s.newFolderPlaceholder : s.newFilePlaceholder}
                className="min-w-0 flex-1 bg-transparent text-[12px] text-text-primary outline-none placeholder:text-text-tertiary"
                onChange={(e) => setCreating({ ...creating, name: e.target.value })}
                onBlur={() => setCreating(undefined)}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') setCreating(undefined);
                  if (e.key !== 'Enter') return;
                  const name = creating.name.trim();
                  if (name === '') return setCreating(undefined);
                  const parent = createParent();
                  void tree.create(parent === '' ? name : `${parent}/${name}`, creating.kind);
                  setCreating(undefined);
                }}
              />
            </div>
          )}
          {openError !== undefined && <ErrorState compact description={openError} />}
          {tree.error !== undefined && <ErrorState compact description={tree.error} />}
          {/* Skeleton while the root lists, then the real tree fades in over
              it. A big repo takes a beat, and the panel used to sit blank and
              then snap — this keeps the whole load one continuous motion. */}
          {tree.loadingRoot ? (
            <TreeSkeleton />
          ) : (
          <div className="motion-safe:animate-[fadeIn_200ms_ease-out]">
          <FileTree
            levels={tree.levels}
            expanded={tree.expanded}
            openPath={file.file?.path}
            selectedPath={selected?.path}
            onToggle={tree.toggle}
            onOpen={(path) => void file.open(path)}
            onSelect={(path, isDir) => {
              setSelected({ path, isDir });
              // Publish the folder so the agent can be told "we're working in
              // here" even when no file is open.
              setSelectedFolder(isDir ? path : undefined);
            }}
          />
          </div>
          )}
        </div>

        {/* Tree/editor divider — same drag mechanism, horizontal direction. */}
        <div
          role="separator"
          aria-orientation="vertical"
          className="w-1 shrink-0 cursor-col-resize border-r border-stroke-tertiary hover:bg-accent/40"
          {...split.handleProps}
        />

        <div className="flex min-w-0 flex-1 flex-col">
          {showChanges ? (
            <ChangesView
              sessionId={sessionId}
              onClose={() => setShowChanges(false)}
              onOpenFile={(path) => {
                setShowChanges(false);
                void file.open(path);
              }}
            />
          ) : (
          <>
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
                {isMarkdown && (
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-pressed={preview}
                    title={preview ? s.showSource : s.showPreview}
                    onClick={() => setPreview((p) => !p)}
                  >
                    {preview ? s.showSource : s.showPreview}
                  </Button>
                )}
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
              <div className="min-h-0 flex-1 overflow-auto">
                {isMarkdown && preview ? (
                  // Preview reads the LIVE draft, not the saved file, so it
                  // reflects edits before they're written.
                  <div className="p-3">
                    <Markdown text={file.draft} />
                  </div>
                ) : (
                <Suspense fallback={<Loader />}>
                  <CodeEditor
                    value={file.draft}
                    language={languageForPath(file.file.path)}
                    readOnly={busy}
                    onChange={file.setDraft}
                    onSelect={setOpenSelection}
                  />
                </Suspense>
                )}
              </div>
            </>
          )}
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
        onShowChanges={() => setShowChanges(true)}
      />
    </aside>
  );
}
