'use client';
// Insights window content — the visual usage explainer on real `insights.get`
// data: token bars (input vs output) + core counters.
import { t } from '@/shared/i18n/t';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { useInsights } from '@/features/butler/viewmodels/useInsights';

const compact = (n: number): string =>
  new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }).format(n);

function Bar({ label, value, max }: { label: string; value: number; max: number }) {
  const pct = max > 0 ? Math.max(2, Math.round((value / max) * 100)) : 2;
  return (
    <div>
      <div className="flex justify-between text-[11px] text-text-tertiary">
        <span>{label}</span>
        <span className="tabular-nums">{compact(value)}</span>
      </div>
      <div className="mt-0.5 h-1.5 overflow-hidden rounded-full bg-hover">
        <div className="h-full rounded-full bg-accent" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

export function InsightsWindow() {
  const s = t().butler.windows;
  const { insights, loading, error } = useInsights();

  if (loading) {
    return (
      <div className="flex justify-center py-3">
        <Loader />
      </div>
    );
  }
  if (error !== undefined) return <ErrorState compact description={error} />;
  if (insights === undefined) return <p className="text-xs text-text-tertiary">{s.insightsEmpty}</p>;

  const max = Math.max(insights.inputTokens, insights.outputTokens);
  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-4 text-center">
        {(
          [
            [s.sessions, insights.sessions],
            [s.turns, insights.turns],
            [s.messages, insights.messages],
          ] as const
        ).map(([label, value]) => (
          <div key={label} className="flex-1">
            <p className="text-base font-semibold tabular-nums text-text-primary">{compact(value)}</p>
            <p className="text-[10px] uppercase tracking-[0.08em] text-text-tertiary">{label}</p>
          </div>
        ))}
      </div>
      <Bar label={s.tokensIn} value={insights.inputTokens} max={max} />
      <Bar label={s.tokensOut} value={insights.outputTokens} max={max} />
    </div>
  );
}
