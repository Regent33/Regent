'use client';
// Skills master list — skills.list {include_archived:true} returns
// {name, description, tags, archived?} per skill (admin_ops.rs::skills_list),
// so retired skills stay visible and can be switched back on. Toggling calls
// skills.opt_out / skills.opt_in — optimistic flip, revert + verbatim error
// on failure.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface SkillRow {
  readonly name: string;
  readonly description?: string;
  readonly tags: readonly string[];
  readonly archived: boolean;
}

export interface SkillsListState {
  readonly skills: readonly SkillRow[];
  readonly loading: boolean;
  readonly error?: string;
  readonly setEnabled: (name: string, enabled: boolean) => void;
}

function toRow(value: unknown): SkillRow | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const name = typeof v.name === 'string' ? v.name : undefined;
  if (name === undefined) return undefined;
  return {
    name,
    description: typeof v.description === 'string' ? v.description : undefined,
    tags: Array.isArray(v.tags) ? v.tags.filter((x): x is string => typeof x === 'string') : [],
    archived: v.archived === true,
  };
}

export function useSkillsList(): SkillsListState {
  const [skills, setSkills] = useState<readonly SkillRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    void deaconRequest('skills.list', { include_archived: true }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setSkills(list.map(toRow).filter((r): r is SkillRow => r !== undefined));
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [reload]);

  const setEnabled = useCallback((name: string, enabled: boolean) => {
    setSkills((prev) => prev.map((s) => (s.name === name ? { ...s, archived: !enabled } : s)));
    void deaconRequest(enabled ? 'skills.opt_in' : 'skills.opt_out', { name }).then((result) => {
      if (!result.ok) {
        setError(result.error.message);
        setReload((n) => n + 1);
      }
    });
  }, []);

  return { skills, loading, error, setEnabled };
}
