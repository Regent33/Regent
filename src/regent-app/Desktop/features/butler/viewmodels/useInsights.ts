'use client';
// Usage rollup for the Insights window — one `insights.get` fetch (real data:
// sessions/turns/tokens across the whole store).
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface Insights {
  readonly sessions: number;
  readonly turns: number;
  readonly turnsOk: number;
  readonly inputTokens: number;
  readonly outputTokens: number;
  readonly messages: number;
}

export interface InsightsState {
  readonly insights?: Insights;
  readonly loading: boolean;
  readonly error?: string;
}

const num = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? v : 0);

export function useInsights(): InsightsState {
  const [state, setState] = useState<InsightsState>({ loading: true });

  useEffect(() => {
    if (!isTauri()) {
      setState({ loading: false });
      return;
    }
    let alive = true;
    void deaconRequest<Record<string, unknown>>('insights.get', {}).then((r) => {
      if (!alive) return;
      if (!r.ok) {
        setState({ loading: false, error: r.error.message });
        return;
      }
      const v = r.value ?? {};
      setState({
        loading: false,
        insights: {
          sessions: num(v.sessions),
          turns: num(v.turns),
          turnsOk: num(v.turns_ok),
          inputTokens: num(v.input_tokens),
          outputTokens: num(v.output_tokens),
          messages: num(v.messages),
        },
      });
    });
    return () => {
      alive = false;
    };
  }, []);

  return state;
}
