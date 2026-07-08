'use client';
// LLM provider API keys — env.list reports the keys (name/label/set-state and a
// masked preview; the raw value is NEVER returned). env.set stores a value and
// returns its masked form; env.unset removes it. Writes are optimistic and
// re-sync from env.list on any error; failures surface verbatim.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export type KeyGroup = 'llm' | 'messaging' | 'search' | 'speech';

const GROUPS: readonly KeyGroup[] = ['llm', 'messaging', 'search', 'speech'];

export interface EnvKey {
  readonly name: string;
  readonly label: string;
  readonly set: boolean;
  readonly masked?: string;
  // Which collapsible section the row belongs to. Missing/unknown (an older
  // deacon that predates grouping) falls back to 'llm' — one flat list.
  readonly group: KeyGroup;
}

export interface ApiKeysState {
  readonly keys: readonly EnvKey[];
  readonly loading: boolean;
  readonly error?: string;
  readonly savingName?: string;
  readonly save: (name: string, value: string) => void;
  readonly remove: (name: string) => void;
}

function toKey(value: unknown): EnvKey | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const name = typeof v.name === 'string' ? v.name : undefined;
  if (name === undefined) return undefined;
  const group = GROUPS.find((g) => g === v.group) ?? 'llm';
  return {
    name,
    label: typeof v.label === 'string' ? v.label : name,
    set: v.set === true,
    masked: typeof v.masked === 'string' ? v.masked : undefined,
    group,
  };
}

export function useApiKeys(): ApiKeysState {
  const [keys, setKeys] = useState<readonly EnvKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [savingName, setSavingName] = useState<string>();
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    void deaconRequest('env.list', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const v = (result.value ?? {}) as Record<string, unknown>;
      const list = Array.isArray(v.keys) ? v.keys : [];
      setKeys(list.map(toKey).filter((k): k is EnvKey => k !== undefined));
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, [reload]);

  const refetch = useCallback(() => setReload((n) => n + 1), []);

  const save = useCallback(
    (name: string, value: string) => {
      setSavingName(name);
      setError(undefined);
      void deaconRequest('env.set', { name, value }).then((result) => {
        setSavingName(undefined);
        if (!result.ok) {
          setError(result.error.message);
          refetch();
          return;
        }
        const v = result.value as Record<string, unknown>;
        const masked = typeof v.masked === 'string' ? v.masked : undefined;
        setKeys((prev) => prev.map((k) => (k.name === name ? { ...k, set: true, masked } : k)));
      });
    },
    [refetch],
  );

  const remove = useCallback(
    (name: string) => {
      setSavingName(name);
      setError(undefined);
      // Optimistic — flip to unset, roll back via refetch if the deacon errors.
      setKeys((prev) => prev.map((k) => (k.name === name ? { ...k, set: false, masked: undefined } : k)));
      void deaconRequest('env.unset', { name }).then((result) => {
        setSavingName(undefined);
        if (!result.ok) {
          setError(result.error.message);
          refetch();
        }
      });
    },
    [refetch],
  );

  return { keys, loading, error, savingName, save, remove };
}
