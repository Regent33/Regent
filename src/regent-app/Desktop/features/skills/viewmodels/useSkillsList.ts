'use client';
// Skills master list — skills.list (admin_ops.rs::skills_list) returns
// {name, description, tags} per skill.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface SkillRow {
  readonly name: string;
  readonly description?: string;
  readonly tags: readonly string[];
}

export interface SkillsListState {
  readonly skills: readonly SkillRow[];
  readonly loading: boolean;
  readonly error?: string;
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
  };
}

export function useSkillsList(): SkillsListState {
  const [state, setState] = useState<SkillsListState>({ skills: [], loading: true });

  useEffect(() => {
    if (!isTauri()) {
      setState({ skills: [], loading: false });
      return;
    }
    let alive = true;
    void deaconRequest('skills.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setState({ skills: [], loading: false, error: result.error.message });
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setState({ skills: list.map(toRow).filter((r): r is SkillRow => r !== undefined), loading: false });
    });
    return () => {
      alive = false;
    };
  }, []);

  return state;
}
