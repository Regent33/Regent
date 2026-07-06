'use client';
// Skill detail — skills.view {name} (admin_ops.rs::skills_view) returns
// {name, description, tags, body, files}; refetches whenever `name` changes.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface SkillDetail {
  readonly name: string;
  readonly description?: string;
  readonly tags: readonly string[];
  readonly body: string;
  readonly files: readonly string[];
}

export interface SkillDetailState {
  readonly detail?: SkillDetail;
  readonly loading: boolean;
  readonly error?: string;
}

function toDetail(value: unknown): SkillDetail | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const name = typeof v.name === 'string' ? v.name : undefined;
  if (name === undefined) return undefined;
  return {
    name,
    description: typeof v.description === 'string' ? v.description : undefined,
    tags: Array.isArray(v.tags) ? v.tags.filter((x): x is string => typeof x === 'string') : [],
    body: typeof v.body === 'string' ? v.body : '',
    files: Array.isArray(v.files) ? v.files.filter((x): x is string => typeof x === 'string') : [],
  };
}

export function useSkillDetail(name: string | undefined): SkillDetailState {
  const [state, setState] = useState<SkillDetailState>({ loading: name !== undefined });

  useEffect(() => {
    if (name === undefined) {
      setState({ loading: false });
      return;
    }
    if (!isTauri()) {
      setState({ loading: false });
      return;
    }
    let alive = true;
    setState({ loading: true });
    void deaconRequest('skills.view', { name }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setState({ loading: false, error: result.error.message });
        return;
      }
      setState({ detail: toDetail(result.value), loading: false });
    });
    return () => {
      alive = false;
    };
  }, [name]);

  return state;
}
