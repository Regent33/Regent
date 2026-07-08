'use client';
// The navigation rail — Hermes IA, Regent-named. Sessions are live over
// `session.list` and route into the chat view; the other nav targets are
// inert until their pages land (M3+). Sessions split into Pinned, the rest,
// and a collapsed Archived group at the bottom (count in its header).
import { useMemo, useState } from 'react';
import { useRouter } from 'next/navigation';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { SearchField } from '@/shared/ui/SearchField';
import { ChevronDownIcon, CodeIcon, FileIcon, MessageIcon, PlusIcon, WrenchIcon } from '@/shared/ui/icons';
import { useSessions, type SessionRow as SessionRowData } from '@/features/shell/viewmodels/useSessions';
import { SessionRow } from '@/features/shell/presentation/SessionRow';
import { open as openOverlay } from '@/shared/state/overlays';

function SectionLabel({ children }: { children: string }) {
  return (
    <p className="px-2.5 pb-1 pt-4 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
      {children}
    </p>
  );
}

// Drag-reorder would need a dependency we're not adding — order is by
// started_at (most recent first) instead, within each group.
function byRecency(a: SessionRowData, b: SessionRowData): number {
  return (b.startedAt ?? '').localeCompare(a.startedAt ?? '');
}

export function LeftRail() {
  const s = t().shell.rail;
  const router = useRouter();
  const { sessions, loading, error, rename, togglePin, toggleArchive, remove } = useSessions();
  const [archivedOpen, setArchivedOpen] = useState(false);
  // Collapsed by default (user call, 2026-07-09) — still shows the 7 newest.
  const [sessionsOpen, setSessionsOpen] = useState(false);
  const [confirmingId, setConfirmingId] = useState<string>();

  const { pinned, regular, archived } = useMemo(() => {
    const live = sessions.filter((row) => !row.archived);
    return {
      pinned: live.filter((row) => row.pinned).sort(byRecency),
      regular: live.filter((row) => !row.pinned).sort(byRecency),
      archived: sessions.filter((row) => row.archived).sort(byRecency),
    };
  }, [sessions]);

  const open = (id: string) => router.push(`/?id=${encodeURIComponent(id)}`);
  const rowLabel = (session: SessionRowData) =>
    session.title ?? `${session.source ?? s.sessionFallback} · ${session.id.replace(/^sess_/, '').slice(0, 6)}`;
  const rowDescription = (session: SessionRowData) =>
    session.messageCount !== undefined ? `${session.messageCount} ${s.messages}` : session.model;

  const onDeleteClick = (id: string) => {
    if (confirmingId === id) {
      remove(id);
      setConfirmingId(undefined);
    } else {
      setConfirmingId(id);
    }
  };

  const renderRow = (session: SessionRowData) => (
    <SessionRow
      key={session.id}
      session={session}
      label={rowLabel(session)}
      description={rowDescription(session)}
      confirmingDelete={confirmingId === session.id}
      onOpen={() => open(session.id)}
      onRename={(title) => rename(session.id, title)}
      onTogglePin={() => togglePin(session.id)}
      onToggleArchive={() => toggleArchive(session.id)}
      onDeleteClick={() => onDeleteClick(session.id)}
    />
  );

  return (
    <nav className="flex w-65 shrink-0 flex-col overflow-clip border-r border-stroke-tertiary p-2">
      {/* Fixed head: nav targets + search never scroll; only the session
          groups below scroll, in their own container. */}
      <ListRow
        icon={<PlusIcon />}
        label={s.newSession}
        trailing={<kbd>{s.newSessionKbd}</kbd>}
        onClick={() => router.push('/')}
      />
      <ListRow icon={<CodeIcon />} label={s.code} onClick={() => router.push('/code')} />
      <ListRow icon={<WrenchIcon />} label={s.skills} onClick={() => openOverlay('skills')} />
      <ListRow icon={<MessageIcon />} label={s.messaging} onClick={() => router.push('/messaging')} />
      <ListRow icon={<FileIcon />} label={s.artifacts} onClick={() => router.push('/artifacts')} />

      <SearchField label={s.searchLabel} placeholder={s.searchPlaceholder} className="mx-1.5 mt-3" />

      <div className="min-h-0 flex-1 overflow-y-auto">
      {pinned.length > 0 && (
        <>
          <SectionLabel>{s.pinned}</SectionLabel>
          {pinned.map(renderRow)}
        </>
      )}

      <button
        type="button"
        className="flex w-full cursor-pointer items-center gap-1 px-2.5 pb-1 pt-4 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary hover:text-text-secondary"
        onClick={() => setSessionsOpen((o) => !o)}
      >
        <ChevronDownIcon className={`size-3 transition-transform ${sessionsOpen ? '' : '-rotate-90'}`} />
        {s.sessions}
      </button>
      {loading && (
        <div className="flex justify-center py-2">
          <Loader />
        </div>
      )}
      {error !== undefined && <ErrorState compact description={error} />}
      {!loading && error === undefined && pinned.length === 0 && regular.length === 0 && (
        <p className="px-2.5 text-xs text-text-tertiary">{s.sessionsEmpty}</p>
      )}
      {/* Collapsed still shows the most recent few — collapse trims, never hides. */}
      {(sessionsOpen ? regular : regular.slice(0, 7)).map(renderRow)}

      {archived.length > 0 && (
        <>
          <button
            type="button"
            className="mt-2 flex w-full cursor-pointer items-center gap-1 px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary hover:text-text-secondary"
            onClick={() => setArchivedOpen((o) => !o)}
          >
            <ChevronDownIcon className={`size-3 transition-transform ${archivedOpen ? '' : '-rotate-90'}`} />
            {s.archived} ({archived.length})
          </button>
          {archivedOpen && archived.map(renderRow)}
        </>
      )}
      </div>
    </nav>
  );
}
