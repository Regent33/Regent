'use client';
// Bottom status strip: live gateway/model from the deacon, ticking session
// timer, version. Placeholders ("—") mark slots whose feeds land later
// (agents, cron, context %) — no fake data.
import { APP_VERSION } from '@/app/config/constants';
import { t } from '@/shared/i18n/t';
import { useStatus } from '@/features/shell/viewmodels/useStatus';

export function StatusBar() {
  const s = t().shell.status;
  const { gatewayReady, model, elapsed } = useStatus();

  return (
    <footer className="flex h-6 shrink-0 select-none items-center gap-4 border-t border-stroke-tertiary px-3 text-[11px] text-text-tertiary">
      <span className="flex items-center gap-1.5">
        <span
          aria-hidden
          className={`size-1.5 rounded-full ${gatewayReady ? 'bg-accent' : 'bg-stroke-primary'}`}
        />
        {gatewayReady ? s.gatewayReady : s.gatewayOffline}
      </span>
      <span>
        {s.agents} {s.placeholder}
      </span>
      <span>
        {s.cron} {s.placeholder}
      </span>
      <span>
        {s.context} {s.placeholder}
      </span>
      <span className="ml-auto tabular-nums">{elapsed}</span>
      <span>{model ?? s.placeholder}</span>
      <span>v{APP_VERSION}</span>
    </footer>
  );
}
