'use client';
// Status-bar state: one `status.get` + `model.get` probe on mount (stale
// results ignored on unmount) and the context-usage percent (from the deacon
// bus, once a turn reports it).
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { useActiveModel, useContextPercent } from '@/shared/state/deaconBus';

export interface StatusState {
  readonly gatewayReady: boolean;
  readonly model?: string;
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

export function useStatus(): StatusState {
  const [gatewayReady, setGatewayReady] = useState(false);
  const [model, setModel] = useState<string | undefined>(undefined);
  const [modelReload, setModelReload] = useState(0);
  const contextPercent = useContextPercent();
  // Live `model.changed` events (model.set, or a new primary applied on the
  // Model page) beat the mount-time probe.
  const busModel = useActiveModel();

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

  return {
    gatewayReady,
    model: busModel ?? model,
    contextPercent,
    refreshModel: () => setModelReload((n) => n + 1),
  };
}
