'use client';
// The navigation rail — Hermes IA, Regent-named. Sessions are live over
// `session.list` and route into the chat view; the other nav targets are
// inert until their pages land (M3+).
import { useRouter } from 'next/navigation';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ListRow } from '@/shared/ui/ListRow';
import { SearchField } from '@/shared/ui/SearchField';
import { FileIcon, MessageIcon, PlusIcon, WrenchIcon } from '@/shared/ui/icons';
import { useSessions } from '@/features/shell/viewmodels/useSessions';

function SectionLabel({ children }: { children: string }) {
  return (
    <p className="px-2.5 pb-1 pt-4 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
      {children}
    </p>
  );
}

export function LeftRail() {
  const s = t().shell.rail;
  const router = useRouter();
  const { sessions, loading, error } = useSessions();

  return (
    <nav className="flex w-65 shrink-0 flex-col overflow-y-auto border-r border-stroke-tertiary p-2">
      <ListRow
        icon={<PlusIcon />}
        label={s.newSession}
        trailing={<kbd>{s.newSessionKbd}</kbd>}
        onClick={() => router.push('/')}
      />
      <ListRow icon={<WrenchIcon />} label={s.skills} />
      <ListRow icon={<MessageIcon />} label={s.messaging} />
      <ListRow icon={<FileIcon />} label={s.artifacts} />

      <SearchField label={s.searchLabel} placeholder={s.searchPlaceholder} className="mx-1.5 mt-3" />

      <SectionLabel>{s.pinned}</SectionLabel>
      <p className="px-2.5 text-xs text-text-tertiary">{s.pinnedHint}</p>

      <SectionLabel>{s.sessions}</SectionLabel>
      {loading && (
        <div className="flex justify-center py-2">
          <Loader />
        </div>
      )}
      {error !== undefined && <ErrorState compact description={error} />}
      {!loading && error === undefined && sessions.length === 0 && (
        <p className="px-2.5 text-xs text-text-tertiary">{s.sessionsEmpty}</p>
      )}
      {sessions.map((session) => (
        <ListRow
          key={session.id}
          label={`${session.source ?? s.sessionFallback} · ${session.id.slice(0, 8)}`}
          description={
            session.messageCount !== undefined ? `${session.messageCount} ${s.messages}` : session.model
          }
          onClick={() => router.push(`/?id=${encodeURIComponent(session.id)}`)}
        />
      ))}
    </nav>
  );
}
