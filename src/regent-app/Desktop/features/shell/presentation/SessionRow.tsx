'use client';
// One rail session row: label + hover-revealed "…" actions popover (Rename,
// Pin/Unpin, Archive/Unarchive, Delete). Not a ListRow — the popover trigger
// is a real sibling button, and nesting a button inside ListRow's own
// <button> would be invalid HTML, so the row's clickable label is a plain
// button next to it instead, sharing the same flat/no-border visual language.
import { useEffect, useRef, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { PinIcon } from '@/shared/ui/icons';
import type { SessionRow as SessionRowData } from '@/features/shell/viewmodels/useSessions';

export interface SessionRowProps {
  readonly session: SessionRowData;
  readonly label: string;
  readonly description?: string;
  readonly confirmingDelete: boolean;
  readonly onOpen: () => void;
  readonly onRename: (title: string) => void;
  readonly onTogglePin: () => void;
  readonly onToggleArchive: () => void;
  readonly onDeleteClick: () => void;
}

export function SessionRow({
  session,
  label,
  description,
  confirmingDelete,
  onOpen,
  onRename,
  onTogglePin,
  onToggleArchive,
  onDeleteClick,
}: SessionRowProps) {
  const s = t().shell.rail;
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(session.title ?? '');
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setMenuOpen(false);
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [menuOpen]);

  const submitRename = () => {
    const next = draft.trim();
    if (next !== '' && next !== session.title) onRename(next);
    setRenaming(false);
  };

  return (
    <div ref={rootRef} className="group relative flex items-center gap-1 rounded-[4px] px-1 py-0.5 hover:bg-hover">
      {renaming ? (
        <input
          autoFocus
          value={draft}
          placeholder={s.renamePlaceholder}
          className="min-w-0 flex-1 border-b border-accent bg-transparent px-1.5 text-[13px] text-text-primary outline-none"
          onChange={(e) => setDraft(e.target.value)}
          onBlur={submitRename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submitRename();
            if (e.key === 'Escape') setRenaming(false);
          }}
        />
      ) : (
        <button
          type="button"
          className="min-w-0 flex-1 cursor-pointer truncate rounded-[4px] px-1.5 py-1 text-left text-[13px] text-text-secondary transition-colors duration-100 hover:text-text-primary"
          onClick={onOpen}
        >
          <span className="flex items-center gap-1">
            {session.pinned && <PinIcon className="size-3 shrink-0 text-text-tertiary" />}
            <span className="truncate">{label}</span>
          </span>
          {description !== undefined && (
            <span className="block truncate text-[11px] text-text-tertiary">{description}</span>
          )}
        </button>
      )}

      <Button
        variant="ghost"
        size="iconSm"
        aria-label={s.actions}
        className={menuOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'}
        onClick={() => setMenuOpen((o) => !o)}
      >
        <span aria-hidden>&hellip;</span>
      </Button>

      {menuOpen && (
        <div
          role="menu"
          className="absolute right-0 top-full z-10 mt-1 w-36 rounded-md border border-stroke-secondary bg-surface py-1 motion-safe:animate-[fadeIn_100ms_ease-out]"
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          <button
            type="button"
            role="menuitem"
            className="block w-full cursor-pointer px-3 py-1.5 text-left text-xs text-text-secondary hover:bg-hover hover:text-text-primary"
            onClick={() => {
              setDraft(session.title ?? '');
              setRenaming(true);
              setMenuOpen(false);
            }}
          >
            {s.rename}
          </button>
          <button
            type="button"
            role="menuitem"
            className="block w-full cursor-pointer px-3 py-1.5 text-left text-xs text-text-secondary hover:bg-hover hover:text-text-primary"
            onClick={() => {
              onTogglePin();
              setMenuOpen(false);
            }}
          >
            {session.pinned ? s.unpin : s.pin}
          </button>
          <button
            type="button"
            role="menuitem"
            className="block w-full cursor-pointer px-3 py-1.5 text-left text-xs text-text-secondary hover:bg-hover hover:text-text-primary"
            onClick={() => {
              onToggleArchive();
              setMenuOpen(false);
            }}
          >
            {session.archived ? s.unarchive : s.archive}
          </button>
          <button
            type="button"
            role="menuitem"
            className={`block w-full cursor-pointer px-3 py-1.5 text-left text-xs hover:bg-hover ${
              confirmingDelete ? 'text-danger' : 'text-text-secondary hover:text-text-primary'
            }`}
            onClick={() => {
              onDeleteClick();
              if (confirmingDelete) setMenuOpen(false);
            }}
          >
            {confirmingDelete ? s.deleteConfirm : s.delete}
          </button>
        </div>
      )}
    </div>
  );
}
