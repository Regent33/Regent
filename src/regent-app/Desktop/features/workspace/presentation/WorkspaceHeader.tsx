'use client';
// The coding panel's title bar: the current root, the folder picker, and the
// panel/maximize/close controls. Extracted from WorkspacePanel, which was
// already past this repo's file ceiling before the bottom panel arrived — this
// block is self-contained and has no state of its own.
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import {
  CloseIcon,
  CollapseIcon,
  ExpandIcon,
  TerminalIcon,
} from '@/shared/ui/icons';
import { folderButtonMode } from '@/features/workspace/domain/workspaceModel';

export interface WorkspaceHeaderProps {
  readonly root: string | undefined;
  readonly isDefault: boolean;
  readonly sessionId: string | undefined;
  /** A folder rebind is in flight — the picker disables and says so. */
  readonly opening: boolean;
  readonly onPickFolder: () => void;
  readonly panelOpen: boolean;
  readonly onTogglePanel: () => void;
  readonly maximized: boolean;
  readonly onToggleMaximize: () => void;
  readonly onClose: () => void;
}

export function WorkspaceHeader({
  root,
  isDefault,
  sessionId,
  opening,
  onPickFolder,
  panelOpen,
  onTogglePanel,
  maximized,
  onToggleMaximize,
  onClose,
}: WorkspaceHeaderProps) {
  const s = t().workspace;
  const changing = folderButtonMode(isDefault) === 'change';
  return (
    <header className="flex items-center gap-1.5 border-b border-stroke-tertiary px-2 py-1.5">
      {/* Windows canonicalization returns the extended-length form
          (\\?\D:\proj); strip it so the header reads like a path a person
          would type. */}
      <span className="flex-1 truncate text-[11px] text-text-tertiary" title={root}>
        {isDefault ? s.sandboxLabel : (root?.replace(/^\\\\\?\\/, '') ?? '')}
      </span>
      {/* Always offered, never only on the scratch space: picking the wrong
          repo has to be recoverable in place. `workspace.set` rebinds a live
          session either way, and the panel remounts on the epoch bump, so the
          old repo's tree, open file and context chip all go with it. */}
      <Button
        size="sm"
        variant="ghost"
        title={
          changing
            ? s.changeFolderHint
            : sessionId === undefined
              ? s.openFolderHint
              : s.openFolderNewChatHint
        }
        disabled={opening}
        onClick={onPickFolder}
      >
        {opening ? s.openingFolder : changing ? s.changeFolder : s.openFolder}
      </Button>
      <Button
        size="iconSm"
        variant="ghost"
        aria-label={s.togglePanel}
        title={s.togglePanel}
        aria-pressed={panelOpen}
        onClick={onTogglePanel}
      >
        <TerminalIcon />
      </Button>
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
  );
}
