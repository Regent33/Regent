'use client';
// Tools list — tools.list (admin_ops.rs::tools_list) returns the full
// session tool catalog: {name, description, toolset, enabled}. `enabled` is
// false when the tool's name is in config `tools.disabled`.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface ToolRow {
  readonly name: string;
  readonly description?: string;
  readonly toolset?: string;
  readonly enabled: boolean;
}

export interface ToolsListState {
  readonly tools: readonly ToolRow[];
  readonly loading: boolean;
  readonly error?: string;
}

function toRow(value: unknown): ToolRow | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const name = typeof v.name === 'string' ? v.name : undefined;
  if (name === undefined) return undefined;
  return {
    name,
    description: typeof v.description === 'string' ? v.description : undefined,
    toolset: typeof v.toolset === 'string' ? v.toolset : undefined,
    enabled: v.enabled !== false,
  };
}

export function useToolsList(): ToolsListState {
  const [state, setState] = useState<ToolsListState>({ tools: [], loading: true });

  useEffect(() => {
    if (!isTauri()) {
      setState({ tools: [], loading: false });
      return;
    }
    let alive = true;
    void deaconRequest('tools.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setState({ tools: [], loading: false, error: result.error.message });
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setState({ tools: list.map(toRow).filter((r): r is ToolRow => r !== undefined), loading: false });
    });
    return () => {
      alive = false;
    };
  }, []);

  return state;
}
