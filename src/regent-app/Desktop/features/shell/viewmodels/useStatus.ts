'use client';
// Status-bar state: one `status.get` + `model.get` probe on mount (stale
// results ignored on unmount), a 1s session timer, and the context-usage
// percent (from the deacon bus, once a turn reports it).
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { useContextPercent } from '@/shared/state/deaconBus';

export interface StatusState {
  readonly gatewayReady: boolean;
  readonly model?: string;
  readonly elapsed: string;
  readonly contextPercent?: number;
  /** Re-probes `model.get` — called after a successful model.set so the
   * label reflects the change without a full status re-fetch. */
  readonly refreshModel: () => void;
}

function readModel(raw: unknown): string | undefined {
  if (typeof raw === 'string' && raw !== '') return raw;
  if (typeof raw === 'object' && raw !== null) {
    const v = raw as Record<string, unknown>;
    for (const key of ['model', 'name', 'id']) {
      if (typeof v[key] === 'string' && v[key] !== '') return v[key] as string;
    }
  }
  return undefined;
}

const mmss = (totalSeconds: number): string => {
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds % 60;
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
};

export function useStatus(): StatusState {
  const [gatewayReady, setGatewayReady] = useState(false);
  const [model, setModel] = useState<string | undefined>(undefined);
  const [seconds, setSeconds] = useState(0);
  const [modelReload, setModelReload] = useState(0);
  const contextPercent = useContextPercent();

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('status.get', {}).then((r) => {
      if (alive) setGatewayReady(r.ok);
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('model.get', {}).then((r) => {
      if (alive && r.ok) setModel(readModel(r.value));
    });
    return () => {
      alive = false;
    };
  }, [modelReload]);

  useEffect(() => {
    const timer = setInterval(() => setSeconds((s) => s + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  return {
    gatewayReady,
    model,
    elapsed: mmss(seconds),
    contextPercent,
    refreshModel: () => setModelReload((n) => n + 1),
  };
}
