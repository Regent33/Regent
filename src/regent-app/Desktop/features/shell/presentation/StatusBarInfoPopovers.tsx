'use client';
// The gateway/agents/cron/context status-bar popovers. Every one is a pure
// display of data the shell already fetches — useStatus's status.get probe,
// useStatusSummary's cron.list/agents.list/status.get polling, and
// deaconBus's usage slice — none of these issue their own RPC call.
import { useState, type ReactNode } from 'react';
import { t } from '@/shared/i18n/t';
import { useCompactionImminent, useContextSnapshot, useUsageSnapshot } from '@/shared/state/deaconBus';
import { useActiveSession } from '@/shared/state/activeSession';
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

const num = (n: number): string => n.toLocaleString();

/** Context fill, plus the compaction landmark. The trigger shows how full the
 * window is — NOT what the turn spent; an agentic turn re-sends the prompt per
 * tool call, so spend/window is a ratio that means nothing and reads past 100%.
 * Spend still earns its place in the panel, labeled as spend. */
export function ContextPopover({ contextPercent }: ContextPopoverProps) {
  const s = t().shell.status;
  const activeSession = useActiveSession();
  const usage = useUsageSnapshot(activeSession);
  const context = useContextSnapshot(activeSession);
  const compactSoon = useCompactionImminent(activeSession);
  const [open, setOpen] = useState(false);
  const compactAt =
    context?.compactAtTokens === undefined
      ? s.contextPanelCompactNever
      : `${num(context.compactAtTokens)} (${Math.round((context.compactAtTokens / context.maxContextTokens) * 100)}%)`;
  return (
    <StatusBarPopover
      open={open}
      onToggle={() => setOpen((o) => !o)}
      onClose={() => setOpen(false)}
      label={s.contextPanelLabel}
      align="right"
      triggerContent={
        // Colour is a hint, never the only signal — the title carries the
        // same warning for screen readers and for anyone who can't see amber.
        <span className={compactSoon ? 'text-amber-500' : undefined} title={compactSoon ? s.contextCompactSoon : s.contextPanelLabel}>
          {s.context} {contextPercent !== undefined ? `${contextPercent}%` : s.placeholder}
        </span>
      }
    >
      <Row label={s.contextPanelUsed} value={context ? num(context.contextTokens) : s.placeholder} />
      <Row label={s.contextPanelSchemas} value={context ? num(context.toolSchemaTokens) : s.placeholder} />
      <Row label={s.contextPanelMax} value={context ? num(context.maxContextTokens) : s.placeholder} />
      <Row label={s.contextPanelCompactAt} value={context ? compactAt : s.placeholder} />
      <Row label={s.contextPanelInput} value={usage ? num(usage.inputTokens) : s.placeholder} />
      <Row label={s.contextPanelOutput} value={usage ? num(usage.outputTokens) : s.placeholder} />
      <Row
        label={s.contextPanelLastRequest}
        value={usage?.lastRequestInputTokens !== undefined ? num(usage.lastRequestInputTokens) : s.placeholder}
      />
      {usage !== undefined && !usage.complete && (
        <p className="mt-1 text-[10px] text-amber-500">{s.contextPanelIncomplete}</p>
      )}
    </StatusBarPopover>
  );
}
