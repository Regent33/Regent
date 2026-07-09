'use client';
// Metadata for the single "default" profile card/detail: the active model
// (model.get -> {model} per admin_ops.rs::model_get) and a skill count
// (skills.list, counting non-archived rows — there's no per-profile scoping
// in the backend yet, so this is the whole install's active skill count).
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface ProfileMetaState {
  readonly model?: string;
  readonly skillCount?: number;
}

function readModel(raw: unknown): string | undefined {
  if (typeof raw !== 'object' || raw === null) return undefined;
  const v = raw as Record<string, unknown>;
  return typeof v.model === 'string' && v.model !== '' ? v.model : undefined;
}

function countActiveSkills(raw: unknown): number | undefined {
  if (!Array.isArray(raw)) return undefined;
  return raw.filter((row) => {
    if (typeof row !== 'object' || row === null) return true;
    return (row as Record<string, unknown>).archived !== true;
  }).length;
}

export function useProfileMeta(): ProfileMetaState {
  const [model, setModel] = useState<string>();
  const [skillCount, setSkillCount] = useState<number>();

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('model.get', {}).then((result) => {
      if (alive && result.ok) setModel(readModel(result.value));
    });
    void deaconRequest('skills.list', { include_archived: true }).then((result) => {
      if (alive && result.ok) setSkillCount(countActiveSkills(result.value));
    });
    return () => {
      alive = false;
    };
  }, []);

  return { model, skillCount };
}
