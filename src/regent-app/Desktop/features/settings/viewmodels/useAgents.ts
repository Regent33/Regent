'use client';
// Agents settings viewmodel — the deacon's agents.* RPCs (list/show/set/
// remove). Errors surface verbatim; a save re-lists so the rail reflects the
// stored truth, not the optimistic draft.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface AgentDef {
  readonly name: string;
  readonly description: string;
  readonly system_prompt: string;
  readonly model: string | null;
  readonly tools: string | null;
}

export interface AgentsState {
  readonly agents: readonly AgentDef[];
  readonly loading: boolean;
  readonly error?: string;
  readonly saving: boolean;
  readonly save: (a: AgentDef) => Promise<boolean>;
  readonly remove: (name: string) => Promise<boolean>;
  readonly reload: () => void;
}

export function useAgents(): AgentsState {
  const [agents, setAgents] = useState<readonly AgentDef[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  const reload = useCallback(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    void deaconRequest('agents.list', {}).then((r) => {
      if (r.ok) {
        setAgents((r.value ?? []) as AgentDef[]);
        setError(undefined);
      } else {
        setError(r.error.message);
      }
      setLoading(false);
    });
  }, []);

  useEffect(reload, [reload]);

  const save = useCallback(
    async (a: AgentDef): Promise<boolean> => {
      setSaving(true);
      const r = await deaconRequest('agents.set', {
        name: a.name,
        description: a.description,
        system_prompt: a.system_prompt,
        model: a.model ?? '',
        tools: a.tools ?? '',
      });
      setSaving(false);
      if (!r.ok) {
        setError(r.error.message);
        return false;
      }
      setError(undefined);
      reload();
      return true;
    },
    [reload],
  );

  const remove = useCallback(
    async (name: string): Promise<boolean> => {
      const r = await deaconRequest('agents.remove', { name });
      if (!r.ok) {
        setError(r.error.message);
        return false;
      }
      setError(undefined);
      reload();
      return true;
    },
    [reload],
  );

  return { agents, loading, error, saving, save, remove, reload };
}
