'use client';
// Status-bar state: one `status.get` + `model.get` probe on mount (stale
// results ignored on unmount) and a 1s session timer.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface StatusState {
  readonly gatewayReady: boolean;
  readonly model?: string;
  readonly elapsed: string;
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

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('status.get', {}).then((r) => {
      if (alive) setGatewayReady(r.ok);
    });
    void deaconRequest('model.get', {}).then((r) => {
      if (alive && r.ok) setModel(readModel(r.value));
    });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    const timer = setInterval(() => setSeconds((s) => s + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  return { gatewayReady, model, elapsed: mmss(seconds) };
}
