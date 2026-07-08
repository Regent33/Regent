'use client';
// The gateway/agents/cron/context status-bar popovers. Every one is a pure
// display of data the shell already fetches — useStatus's status.get probe,
// useStatusSummary's cron.list/agents.list/status.get polling, and
// deaconBus's usage slice — none of these issue their own RPC call.
import { useState, type ReactNode } from 'react';
import { t } from '@/shared/i18n/t';
import { useUsageSnapshot } from '@/shared/state/deaconBus';
import { StatusBarPopover } from '@/features/shell/presentation/StatusBarPopover';

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-0.5 text-xs">
      <span className="text-text-tertiary">{label}</span>
      <span className="tabular-nums text-text-primary">{value}</span>
    </div>
  );
}

function formatEpoch(seconds: number | undefined, never: string): string {
  if (seconds === undefined) return never;
  return new Date(seconds * 1000).toLocaleString();
}

export interface GatewayPopoverProps {
  readonly gatewayReady: boolean;
  readonly model?: string;
  readonly activeSessions?: number;
  readonly cronEnabled?: number;
  readonly cronTotal?: number;
}

export function GatewayPopover({ gatewayReady, model, activeSessions, cronEnabled, cronTotal }: GatewayPopoverProps) {
  const s = t().shell.status;
  const [open, setOpen] = useState(false);
  return (
    <StatusBarPopover
      open={open}
      onToggle={() => setOpen((o) => !o)}
      onClose={() => setOpen(false)}
      label={s.gatewayPanelLabel}
      triggerContent={
        <span className="flex items-center gap-1.5">
          <span aria-hidden className={`size-1.5 rounded-full ${gatewayReady ? 'bg-accent' : 'bg-stroke-primary'}`} />
          {gatewayReady ? s.gatewayReady : s.gatewayOffline}
        </span>
      }
    >
      <Row label={s.gatewayPanelModel} value={model ?? s.placeholder} />
      <Row label={s.gatewayPanelSessions} value={activeSessions ?? s.placeholder} />
      <Row
        label={s.gatewayPanelCron}
        value={cronTotal !== undefined ? `${cronEnabled ?? 0}/${cronTotal}` : s.placeholder}
      />
    </StatusBarPopover>
  );
}

export interface AgentsPopoverProps {
  readonly agentsCount?: number;
  readonly activeSessions?: number;
}

export function AgentsPopover({ agentsCount, activeSessions }: AgentsPopoverProps) {
  const s = t().shell.status;
  const [open, setOpen] = useState(false);
  return (
    <StatusBarPopover
      open={open}
      onToggle={() => setOpen((o) => !o)}
      onClose={() => setOpen(false)}
      label={s.agentsPanelLabel}
      triggerContent={`${s.agents} ${agentsCount ?? s.placeholder}`}
    >
      <Row label={s.agentsPanelCount} value={agentsCount ?? s.placeholder} />
      <Row label={s.agentsPanelActive} value={activeSessions ?? s.placeholder} />
    </StatusBarPopover>
  );
}

export interface CronPopoverProps {
  readonly cronEnabled?: number;
  readonly cronTotal?: number;
  readonly cronNextRunAt?: number;
}

export function CronPopover({ cronEnabled, cronTotal, cronNextRunAt }: CronPopoverProps) {
  const s = t().shell.status;
  const cronText = t().cron;
  const [open, setOpen] = useState(false);
  return (
    <StatusBarPopover
      open={open}
      onToggle={() => setOpen((o) => !o)}
      onClose={() => setOpen(false)}
      label={s.cronPanelLabel}
      triggerContent={`${s.cron} ${cronTotal !== undefined ? `${cronEnabled ?? 0}/${cronTotal}` : s.placeholder}`}
    >
      <Row label={s.cronPanelEnabled} value={cronEnabled ?? s.placeholder} />
      <Row label={s.cronPanelTotal} value={cronTotal ?? s.placeholder} />
      <Row label={cronText.nextRun} value={formatEpoch(cronNextRunAt, cronText.never)} />
    </StatusBarPopover>
  );
}

export interface ContextPopoverProps {
  readonly contextPercent?: number;
}

export function ContextPopover({ contextPercent }: ContextPopoverProps) {
  const s = t().shell.status;
  const usage = useUsageSnapshot();
  const [open, setOpen] = useState(false);
  return (
    <StatusBarPopover
      open={open}
      onToggle={() => setOpen((o) => !o)}
      onClose={() => setOpen(false)}
      label={s.contextPanelLabel}
      align="right"
      triggerContent={`${s.context} ${contextPercent !== undefined ? `${contextPercent}%` : s.placeholder}`}
    >
      <Row label={s.contextPanelInput} value={usage?.inputTokens ?? s.placeholder} />
      <Row label={s.contextPanelOutput} value={usage?.outputTokens ?? s.placeholder} />
      <Row label={s.contextPanelMax} value={usage?.contextMax ?? s.placeholder} />
    </StatusBarPopover>
  );
}
