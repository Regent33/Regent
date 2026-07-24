'use client';
// Status-bar update feed (ADR-041, Phase 0 — notify only): one cached,
// fail-silent `update.status` probe on mount. An old deacon without the method,
// a transport error, or a malformed body all leave the verdict null, so the
// badge simply never appears. The deacon answers from its background checker's
// cache — this never triggers a network fetch.
import { useEffect, useState } from 'react';
import { type UpdateStatus, parseUpdateStatus } from '@/features/shell/domain/updateStatus';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export function useUpdateStatus(): UpdateStatus | null {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('update.status', {}).then((r) => {
      if (alive && r.ok) setStatus(parseUpdateStatus(r.value));
    });
    return () => {
      alive = false;
    };
  }, []);
  return status;
}
