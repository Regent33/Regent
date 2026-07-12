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
import { useVoiceHealth } from '@/features/shell/viewmodels/useVoiceHealth';
import { useModelMenu } from '@/features/shell/viewmodels/useModelMenu';
import { StatusBarModelMenu } from '@/features/shell/presentation/StatusBarModelMenu';
import {
  AgentsPopover,
  ContextPopover,
  CronPopover,
  GatewayPopover,
} from '@/features/shell/presentation/StatusBarInfoPopovers';

// Small ASR/TTS warmup dot: green when models are loaded + warm, amber-pulsing
// while the server is answering but still warming, gray when it isn't running.
function VoiceIndicator() {
  const s = t().shell.status;
  const health = useVoiceHealth();
  const label = health === 'ready' ? s.voiceReady : health === 'warming' ? s.voiceWarming : s.voiceOffline;
  const dot =
    health === 'ready' ? 'bg-accent' : health === 'warming' ? 'bg-amber-500 animate-pulse' : 'bg-stroke-primary';
  return (
    <span className="flex items-center gap-1.5" title={`${s.voiceTitle} — ${label}`}>
      <span aria-hidden className={`size-1.5 rounded-full ${dot}`} />
      {label}
    </span>
  );
}

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
      <VoiceIndicator />
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
