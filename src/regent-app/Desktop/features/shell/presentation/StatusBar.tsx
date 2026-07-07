'use client';
// Bottom status strip: live gateway/model/cron/agents/context from the
// deacon, ticking session timer, version. Placeholders ("—") mark slots
// whose feed hasn't reported yet — no fake data.
import { APP_VERSION } from '@/app/config/constants';
import { t } from '@/shared/i18n/t';
import { useStatus } from '@/features/shell/viewmodels/useStatus';
import { useStatusSummary } from '@/features/shell/viewmodels/useStatusSummary';
import { useModelMenu } from '@/features/shell/viewmodels/useModelMenu';
import { StatusBarModelMenu } from '@/features/shell/presentation/StatusBarModelMenu';

export function StatusBar() {
  const s = t().shell.status;
  const { gatewayReady, model, elapsed, contextPercent, refreshModel } = useStatus();
  const { cronEnabled, cronTotal, agentsCount } = useStatusSummary();
  const modelMenu = useModelMenu(refreshModel);

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
        {s.agents} {agentsCount ?? s.placeholder}
      </span>
      <span>
        {s.cron} {cronTotal !== undefined ? `${cronEnabled ?? 0}/${cronTotal}` : s.placeholder}
      </span>
      <span>
        {s.context} {contextPercent !== undefined ? `${contextPercent}%` : s.placeholder}
      </span>
      <span className="ml-auto tabular-nums">{elapsed}</span>
      <StatusBarModelMenu menu={modelMenu} label={model ?? s.placeholder} />
      <span>v{APP_VERSION}</span>
    </footer>
  );
}
