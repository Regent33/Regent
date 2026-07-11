'use client';
// Bottom status strip: live gateway/model/cron/agents/context from the
// deacon, version. Placeholders ("—") mark slots whose feed hasn't reported
// yet — no fake data.
import { APP_VERSION } from '@/app/config/constants';
import { t } from '@/shared/i18n/t';
import { useRouter } from '@/shared/infrastructure/router/adapter';
import { GraphIcon } from '@/shared/ui/icons';
import { useStatus } from '@/features/shell/viewmodels/useStatus';
import { useStatusSummary } from '@/features/shell/viewmodels/useStatusSummary';
import { useModelMenu } from '@/features/shell/viewmodels/useModelMenu';
import { StatusBarModelMenu } from '@/features/shell/presentation/StatusBarModelMenu';
import {
  AgentsPopover,
  ContextPopover,
  CronPopover,
  GatewayPopover,
} from '@/features/shell/presentation/StatusBarInfoPopovers';

export function StatusBar() {
  const s = t().shell.status;
  const router = useRouter();
  const { gatewayReady, model, contextPercent, refreshModel } = useStatus();
  const { cronEnabled, cronTotal, cronNextRunAt, agentsCount, activeSessions } = useStatusSummary();
  const modelMenu = useModelMenu(refreshModel);

  return (
    <footer className="flex h-6 shrink-0 select-none items-center gap-4 border-t border-stroke-tertiary px-3 text-[11px] text-text-tertiary">
      <GatewayPopover
        gatewayReady={gatewayReady}
        model={model}
        activeSessions={activeSessions}
        cronEnabled={cronEnabled}
        cronTotal={cronTotal}
      />
      <AgentsPopover agentsCount={agentsCount} activeSessions={activeSessions} />
      <CronPopover cronEnabled={cronEnabled} cronTotal={cronTotal} cronNextRunAt={cronNextRunAt} />
      <ContextPopover contextPercent={contextPercent} />
      <button
        type="button"
        aria-label={t().graph.open}
        title={t().graph.open}
        className="flex cursor-pointer items-center gap-1 hover:text-text-secondary"
        onClick={() => router.push('/graph')}
      >
        <GraphIcon className="size-3.5" />
        {t().graph.statusLabel}
      </button>
      <span className="ml-auto" />
      <StatusBarModelMenu menu={modelMenu} label={model ?? s.placeholder} />
      <span>v{APP_VERSION}</span>
    </footer>
  );
}
