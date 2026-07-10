'use client';
// Messaging — platform conversations that arrived through the gateway
// (Telegram, Slack, …), grouped per platform with a SESSIONS-style collapse
// (collapsed by default). EVERY known platform is listed — connected or not —
// so the surface doubles as a map of what can be wired up. Local/internal
// sources stay out; platform *setup* is key management (Settings → API Keys).
import { useMemo, useState } from 'react';
import { useRouter } from '@/shared/infrastructure/router/adapter';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { ChevronDownIcon } from '@/shared/ui/icons';
import { useSessions, type SessionRow } from '@/features/shell/viewmodels/useSessions';

// Everything the app itself creates; anything else came in over a platform.
const LOCAL_SOURCES = new Set(['deacon', 'daemon', 'delegate', 'review', 'curator', 'cron', 'board']);

// The gateway's platform surface — shown even with zero conversations.
const KNOWN_PLATFORMS = [
  'telegram',
  'discord',
  'slack',
  'whatsapp',
  'messenger',
  'line',
  'mattermost',
  'twilio',
  'teams',
  'feishu',
  'wechat',
  'wecom',
  'email',
  'jira',
  'azure_devops',
  'trello',
  'gchat',
] as const;

export function MessagingView() {
  const s = t().messaging;
  const router = useRouter();
  const { sessions, loading, error } = useSessions();
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  const byPlatform = useMemo(() => {
    const groups = new Map<string, SessionRow[]>(KNOWN_PLATFORMS.map((p) => [p, []]));
    for (const session of sessions) {
      const source = session.source ?? '';
      if (source === '' || LOCAL_SOURCES.has(source)) continue;
      const list = groups.get(source) ?? [];
      list.push(session);
      groups.set(source, list);
    }
    // Platforms with conversations first (by count desc), the rest alphabetical.
    return [...groups.entries()].sort(
      ([a, ra], [b, rb]) => rb.length - ra.length || a.localeCompare(b),
    );
  }, [sessions]);

  const toggle = (platform: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(platform)) next.delete(platform);
      else next.add(platform);
      return next;
    });

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
        {!loading &&
          error === undefined &&
          byPlatform.map(([platform, rows]) => {
            const open = expanded.has(platform);
            return (
              <section key={platform} className="mb-1.5">
                <button
                  type="button"
                  aria-expanded={open}
                  className="flex w-full cursor-pointer items-center gap-1 py-1.5 text-left text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary hover:text-text-secondary"
                  onClick={() => toggle(platform)}
                >
                  <ChevronDownIcon className={`size-3 shrink-0 transition-transform ${open ? '' : '-rotate-90'}`} />
                  {platform} · {rows.length}
                </button>
                {open &&
                  (rows.length === 0 ? (
                    <p className="px-4 pb-2 text-xs text-text-tertiary">{s.platformEmpty}</p>
                  ) : (
                    rows.map((session) => (
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
                    ))
                  ))}
              </section>
            );
          })}
      </div>
    </div>
  );
}
