'use client';
// Main models — the Primary + ordered Fallbacks chain the deacon runs chat
// through (config `agents_defaults`). Each entry is a {provider, model} where
// provider is a NAME from config.providers (carrying its base_url + key). Reads
// config.get once; writes the whole primary/fallbacks value via the validated
// config.set. Applies on the next deacon start (its note renders verbatim).
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface ModelRef {
  readonly provider: string;
  readonly model: string;
}
export interface ProviderOption {
  readonly name: string;
  readonly models: readonly string[];
}

export interface MainModelsState {
  readonly providers: readonly ProviderOption[];
  readonly primary?: ModelRef;
  readonly fallbacks: readonly ModelRef[];
  readonly loading: boolean;
  readonly error?: string;
  readonly note?: string;
  readonly setPrimary: (ref: ModelRef) => void;
  readonly setFallbacks: (refs: readonly ModelRef[]) => void;
}

function toRef(v: unknown): ModelRef | undefined {
  if (typeof v !== 'object' || v === null) return undefined;
  const o = v as Record<string, unknown>;
  return typeof o.provider === 'string' && typeof o.model === 'string'
    ? { provider: o.provider, model: o.model }
    : undefined;
}

export function useMainModels(): MainModelsState {
  const [providers, setProviders] = useState<readonly ProviderOption[]>([]);
  const [primary, setPrimaryState] = useState<ModelRef>();
  const [fallbacks, setFallbacksState] = useState<readonly ModelRef[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [note, setNote] = useState<string>();

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    void deaconRequest('config.get', {}).then((r) => {
      if (!alive) return;
      if (!r.ok) {
        setError(r.error.message);
        setLoading(false);
        return;
      }
      const cfg = r.value as Record<string, unknown>;
      const map = (cfg.providers ?? {}) as Record<string, { models?: unknown }>;
      setProviders(
        Object.entries(map).map(([name, spec]) => ({
          name,
          models: Array.isArray(spec?.models)
            ? spec.models.filter((m): m is string => typeof m === 'string')
            : [],
        })),
      );
      const ad = (cfg.agents_defaults ?? {}) as Record<string, unknown>;
      setPrimaryState(toRef(ad.primary));
      setFallbacksState(
        Array.isArray(ad.fallbacks)
          ? ad.fallbacks.map(toRef).filter((x): x is ModelRef => x !== undefined)
          : [],
      );
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  const write = useCallback((path: string, value: unknown, apply: () => void) => {
    apply(); // optimistic
    void deaconRequest('config.set', { path, value }).then((r) => {
      if (!r.ok) setError(r.error.message);
      else setNote((r.value as { note?: string }).note);
    });
  }, []);

  const setPrimary = useCallback(
    (ref: ModelRef) => write('agents_defaults.primary', ref, () => setPrimaryState(ref)),
    [write],
  );
  const setFallbacks = useCallback(
    (refs: readonly ModelRef[]) =>
      write('agents_defaults.fallbacks', refs, () => setFallbacksState(refs)),
    [write],
  );

  return { providers, primary, fallbacks, loading, error, note, setPrimary, setFallbacks };
}
