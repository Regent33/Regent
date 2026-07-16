'use client';
// Profile list + active-profile state over the deacon's profile.* RPCs
// (profile.list / profile.create / profile.switch). Mutations re-list from
// the backend — the store is the source of truth, never optimistic state.
// Persona editors keyed by `active` re-fetch after a switch, which is what
// makes "saves in real time" hold per profile.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, deaconRequestRetry, isTauri } from '@/shared/infrastructure/rpc/client';

export interface ProfilesState {
  readonly profiles: readonly string[];
  readonly active: string;
  readonly error?: string;
  create(name: string): Promise<boolean>;
  switchTo(name: string): Promise<void>;
}

function readList(raw: unknown): { profiles: string[]; active: string } | undefined {
  if (typeof raw !== 'object' || raw === null) return undefined;
  const v = raw as Record<string, unknown>;
  if (!Array.isArray(v.profiles) || typeof v.active !== 'string') return undefined;
  return { profiles: v.profiles.filter((p): p is string => typeof p === 'string'), active: v.active };
}

export function useProfiles(): ProfilesState {
  const [profiles, setProfiles] = useState<readonly string[]>(['default']);
  const [active, setActive] = useState('default');
  const [error, setError] = useState<string>();

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    // Retrying load — first-launch race against deacon spawn (see usePersona).
    const result = await deaconRequestRetry('profile.list', {});
    const parsed = result.ok ? readList(result.value) : undefined;
    if (parsed) {
      setProfiles(parsed.profiles);
      setActive(parsed.active);
      setError(undefined);
    } else if (!result.ok) {
      setError(result.error.message);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const create = useCallback(
    async (name: string): Promise<boolean> => {
      const result = await deaconRequest('profile.create', { name });
      if (!result.ok) {
        setError(result.error.message);
        return false;
      }
      await refresh();
      return true;
    },
    [refresh],
  );

  const switchTo = useCallback(
    async (name: string) => {
      const result = await deaconRequest('profile.switch', { name });
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      await refresh();
    },
    [refresh],
  );

  return { profiles, active, error, create, switchTo };
}
