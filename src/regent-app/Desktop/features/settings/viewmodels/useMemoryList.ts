'use client';
// Memory & Context list — memory.list rows; memory.pin/memory.unpin toggle a
// row, memory.forget removes it. The two-click "are you sure" confirm for
// forget is presentation-layer state (this hook just executes once asked).
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface MemoryNode {
  readonly id: string;
  readonly kind?: string;
  readonly name?: string;
  readonly content?: string;
  readonly pinned: boolean;
}

export interface MemoryListState {
  readonly nodes: readonly MemoryNode[];
  readonly loading: boolean;
  readonly error?: string;
  readonly togglePin: (node: MemoryNode) => void;
  readonly forget: (id: string) => void;
}

function toNode(value: unknown): MemoryNode | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.id === 'string' ? v.id : undefined;
  if (id === undefined) return undefined;
  return {
    id,
    kind: typeof v.kind === 'string' ? v.kind : undefined,
    name: typeof v.name === 'string' ? v.name : undefined,
    content: typeof v.content === 'string' ? v.content : undefined,
    pinned: v.pinned === true,
  };
}

export function useMemoryList(): MemoryListState {
  const [nodes, setNodes] = useState<readonly MemoryNode[]>([]);
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
    void deaconRequest('memory.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setNodes(list.map(toNode).filter((n): n is MemoryNode => n !== undefined));
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [reload]);

  const refetch = useCallback(() => setReload((n) => n + 1), []);

  const togglePin = useCallback(
    (node: MemoryNode) => {
      const method = node.pinned ? 'memory.unpin' : 'memory.pin';
      setNodes((prev) => prev.map((n) => (n.id === node.id ? { ...n, pinned: !n.pinned } : n)));
      void deaconRequest(method, { id: node.id }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  const forget = useCallback(
    (id: string) => {
      setNodes((prev) => prev.filter((n) => n.id !== id));
      void deaconRequest('memory.forget', { id }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  return { nodes, loading, error, togglePin, forget };
}
