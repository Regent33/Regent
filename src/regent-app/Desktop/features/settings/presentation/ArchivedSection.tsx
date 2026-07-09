'use client';
// Archived — the same useSessions viewmodel the left rail uses (session.list,
// session.archive, session.delete), filtered to archived === true. Unarchive
// flips the same toggle the rail's "…" menu uses; Delete is a two-click
// confirm, mirroring MemorySection's Forget pattern.
import { useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { useSessions, type SessionRow } from '@/features/shell/viewmodels/useSessions';

function label(session: SessionRow, fallback: string): string {
  return session.title ?? `${session.source ?? fallback} · ${session.id.replace(/^sess_/, '').slice(0, 6)}`;
}

export function ArchivedSection() {
  const s = t().settings.archived;
  const rail = t().shell.rail;
  const { sessions, loading, error, toggleArchive, remove } = useSessions();
  const [confirmingId, setConfirmingId] = useState<string>();
  const archived = sessions.filter((session) => session.archived);

  const onDelete = (id: string) => {
    if (confirmingId === id) {
      remove(id);
      setConfirmingId(undefined);
    } else {
      setConfirmingId(id);
    }
  };

  return (
    <Section title={s.title}>
      {loading && <Loader />}
      {error !== undefined && <ErrorState description={error} />}
      {!loading && error === undefined && archived.length === 0 && <EmptyState title={s.empty} />}
      {archived.map((session) => (
        <div key={session.id} className="flex items-center gap-2.5 border-b border-stroke-tertiary py-2 last:border-b-0">
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm text-text-primary">{label(session, rail.sessionFallback)}</p>
            {session.messageCount !== undefined && (
              <p className="truncate text-xs text-text-tertiary">
                {session.messageCount} {rail.messages}
              </p>
            )}
          </div>
          <Button variant="secondary" size="sm" onClick={() => toggleArchive(session.id)}>
            {rail.unarchive}
          </Button>
          <Button variant={confirmingId === session.id ? 'default' : 'ghost'} size="sm" onClick={() => onDelete(session.id)}>
            {confirmingId === session.id ? rail.deleteConfirm : rail.delete}
          </Button>
        </div>
      ))}
    </Section>
  );
}
