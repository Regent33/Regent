'use client';
// Titlebar session menu (Hermes parity): shows "New Conversation" or the
// active session's title, with Pin / Copy ID / Export / Rename / Archive /
// Delete. Session data + mutations come from useSessions (optimistic, same
// store the rail uses); the active id from the ChatView-published store.
import { useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ChevronDownIcon } from '@/shared/ui/icons';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { useActiveSession } from '@/shared/state/activeSession';
import { useSessions } from '@/features/shell/viewmodels/useSessions';

async function exportSession(id: string, title: string): Promise<void> {
  const history = await deaconRequest('session.history', { session_id: id });
  if (!history.ok) return;
  const blob = new Blob([JSON.stringify({ session_id: id, title, messages: history.value }, null, 2)], {
    type: 'application/json',
  });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = `${title.replaceAll(/[^\w-]+/g, '_') || id}.json`;
  a.click();
  URL.revokeObjectURL(a.href);
}

export function SessionTitleMenu() {
  const s = t().shell.titlebar.sessionMenu;
  const router = useRouter();
  const activeId = useActiveSession();
  const { sessions, rename, togglePin, toggleArchive, remove } = useSessions();
  const [open, setOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState('');
  const [confirming, setConfirming] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const session = sessions.find((row) => row.id === activeId);
  const label = session?.title ?? (activeId !== undefined ? activeId.replace(/^sess_/, '').slice(0, 8) : s.newConversation);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  useEffect(() => {
    setConfirming(false);
    setRenaming(false);
  }, [activeId, open]);

  const item = (labelText: string, onClick: () => void, danger = false) => (
    <button
      type="button"
      role="menuitem"
      className={`block w-full cursor-pointer px-3 py-1.5 text-left text-xs hover:bg-hover ${
        danger ? 'text-danger' : 'text-text-secondary hover:text-text-primary'
      }`}
      onClick={onClick}
    >
      {labelText}
    </button>
  );

  return (
    <div ref={rootRef} className="relative flex items-center">
      <Button
        variant="text"
        size="sm"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => activeId !== undefined && setOpen((o) => !o)}
      >
        <span className="max-w-64 truncate text-sm">{label}</span>
        {activeId !== undefined && <ChevronDownIcon className="size-3.5" />}
      </Button>

      {open && session !== undefined && (
        <div
          role="menu"
          className="absolute left-0 top-full z-20 mt-1 w-44 rounded-md border border-stroke-secondary bg-surface py-1 motion-safe:animate-[fadeIn_100ms_ease-out]"
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          {renaming ? (
            <input
              autoFocus
              value={draft}
              placeholder={s.renamePlaceholder}
              className="mx-2 my-1 w-[calc(100%-1rem)] border-b border-accent bg-transparent px-1 text-xs text-text-primary outline-none"
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  const next = draft.trim();
                  if (next !== '') rename(session.id, next);
                  setRenaming(false);
                  setOpen(false);
                }
                if (e.key === 'Escape') setRenaming(false);
              }}
            />
          ) : (
            item(s.rename, () => {
              setDraft(session.title ?? '');
              setRenaming(true);
            })
          )}
          {item(session.pinned ? s.unpin : s.pin, () => {
            togglePin(session.id);
            setOpen(false);
          })}
          {item(s.copyId, () => {
            void navigator.clipboard.writeText(session.id);
            setOpen(false);
          })}
          {item(s.export, () => {
            void exportSession(session.id, session.title ?? session.id);
            setOpen(false);
          })}
          {item(session.archived ? s.unarchive : s.archive, () => {
            toggleArchive(session.id);
            setOpen(false);
          })}
          {item(
            confirming ? s.deleteConfirm : s.delete,
            () => {
              if (!confirming) {
                setConfirming(true);
                return;
              }
              remove(session.id);
              setOpen(false);
              router.push('/');
            },
            true,
          )}
        </div>
      )}
    </div>
  );
}
