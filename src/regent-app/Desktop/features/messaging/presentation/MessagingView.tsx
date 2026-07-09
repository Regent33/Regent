'use client';
// Messaging — platform conversations that arrived through the gateway
// (Telegram, Slack, …): every session whose source is a platform, grouped by
// platform, opening in the chat view. Local/internal sources stay out — the
// rail owns those. Platform *setup* is key management (Settings → API Keys).
import { useMemo } from 'react';
import { useRouter } from 'next/navigation';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { useSessions, type SessionRow } from '@/features/shell/viewmodels/useSessions';

// Everything the app itself creates; anything else came in over a platform.
const LOCAL_SOURCES = new Set(['deacon', 'daemon', 'delegate', 'review', 'curator', 'cron', 'board']);

export function MessagingView() {
  const s = t().messaging;
  const router = useRouter();
  const { sessions, loading, error } = useSessions();

  const byPlatform = useMemo(() => {
    const groups = new Map<string, SessionRow[]>();
    for (const session of sessions) {
      const source = session.source ?? '';
      if (source === '' || LOCAL_SOURCES.has(source)) continue;
      const list = groups.get(source) ?? [];
      list.push(session);
      groups.set(source, list);
    }
    return [...groups.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [sessions]);

  return (
    <div className="mx-auto flex h-full max-w-190 flex-col px-6 py-6">
      <h1 className="text-lg font-semibold text-text-primary">{s.title}</h1>
      <p className="mt-1 text-xs text-text-tertiary">{s.subtitle}</p>

      <div className="mt-4 min-h-0 flex-1 overflow-y-auto">
        {loading && (
          <div className="flex justify-center py-6">
            <Loader />
          </div>
        )}
        {error !== undefined && <ErrorState compact description={error} />}
        {!loading && error === undefined && byPlatform.length === 0 && (
          <EmptyState title={s.empty} hint={s.emptyHint} />
        )}
        {byPlatform.map(([platform, rows]) => (
          <section key={platform} className="mb-5">
            <h2 className="pb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
              {platform} · {rows.length}
            </h2>
            {rows.map((session) => (
              <button
                key={session.id}
                type="button"
                className="flex w-full cursor-pointer items-baseline gap-2 rounded-md px-2.5 py-2 text-left transition-colors hover:bg-hover"
                onClick={() => router.push(`/?id=${encodeURIComponent(session.id)}`)}
              >
                <span className="min-w-0 flex-1 truncate text-sm text-text-primary">
                  {session.title ?? session.id.replace(/^sess_/, '').slice(0, 8)}
                </span>
                {session.messageCount !== undefined && (
                  <span className="shrink-0 text-xs text-text-tertiary">
                    {session.messageCount} {s.messages}
                  </span>
                )}
              </button>
            ))}
          </section>
        ))}
      </div>
    </div>
  );
}
