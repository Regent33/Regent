'use client';
// Pending memory writes awaiting approval — memory.pending lists them,
// memory.approve/memory.reject resolve one (approve promotes it to a memory
// node; reject discards it). Both are one-click (the write was already
// staged by the agent, unlike the destructive Forget in useMemoryList).
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface PendingMemoryWrite {
  readonly id: string;
  readonly kind?: string;
  readonly name?: string;
  readonly content?: string;
  readonly trust?: string;
}

export interface MemoryPendingState {
  readonly pending: readonly PendingMemoryWrite[];
  readonly loading: boolean;
  readonly error?: string;
  readonly approve: (id: string) => void;
  readonly reject: (id: string) => void;
}

function toPending(value: unknown): PendingMemoryWrite | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.id === 'string' ? v.id : undefined;
  if (id === undefined) return undefined;
  return {
    id,
    kind: typeof v.kind === 'string' ? v.kind : undefined,
    name: typeof v.name === 'string' ? v.name : undefined,
    content: typeof v.content === 'string' ? v.content : undefined,
    trust: typeof v.trust === 'string' ? v.trust : undefined,
  };
}

export function useMemoryPending(): MemoryPendingState {
  const [pending, setPending] = useState<readonly PendingMemoryWrite[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    void deaconRequest('memory.pending', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setPending(list.map(toPending).filter((p): p is PendingMemoryWrite => p !== undefined));
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [reload]);

  const resolve = useCallback((method: string, id: string) => {
    setPending((prev) => prev.filter((p) => p.id !== id));
    void deaconRequest(method, { id }).then((result) => {
      if (!result.ok) {
        setError(result.error.message);
        setReload((n) => n + 1);
      }
    });
  }, []);

  const approve = useCallback((id: string) => resolve('memory.approve', id), [resolve]);
  const reject = useCallback((id: string) => resolve('memory.reject', id), [resolve]);

  return { pending, loading, error, approve, reject };
}
