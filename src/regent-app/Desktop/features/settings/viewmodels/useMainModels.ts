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
  /** Pins this ref to a stored key slot (2..8; 1/undefined = the base key).
   *  Lets a fallback be the same provider+model on a different key. */
  readonly key_slot?: number;
}
export interface ProviderOption {
  readonly name: string;
  readonly models: readonly string[];
  /** The env var this provider reads its key from (config api_key_env). */
  readonly keyEnv?: string;
}

export interface KeySlot {
  readonly slot: number;
  readonly masked?: string;
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
  /** Stored key slots for a provider (slot 1 = active); >1 enables the picker. */
  readonly keySlotsFor: (provider: string) => readonly KeySlot[];
  /** Swap numbered slot N into the provider's active key (env.activate). */
  readonly activateKey: (provider: string, slot: number) => void;
}

function toRef(v: unknown): ModelRef | undefined {
  if (typeof v !== 'object' || v === null) return undefined;
  const o = v as Record<string, unknown>;
  if (typeof o.provider !== 'string' || typeof o.model !== 'string') return undefined;
  return {
    provider: o.provider,
    model: o.model,
    ...(typeof o.key_slot === 'number' && o.key_slot > 1 ? { key_slot: o.key_slot } : {}),
  };
}

/** A ref with `slot` pinned — slot 1 (the base key) is the unpinned form, so
 *  it's omitted and the serialized config stays byte-identical to before. */
export function withKeySlot(ref: ModelRef, slot: number): ModelRef {
  const { key_slot: _drop, ...bare } = ref;
  return slot > 1 ? { ...bare, key_slot: slot } : bare;
}

interface EnvRow {
  readonly name: string;
  readonly set: boolean;
  readonly masked?: string;
}

/** Effective model catalogs per provider name from the deacon `providers.models`
 *  op. Empty when the op is missing (older deacon) or errors — the caller then
 *  falls back to config-listed models only, surfacing no error. */
async function fetchCatalog(): Promise<Record<string, readonly string[]>> {
  const r = await deaconRequest('providers.models', {});
  if (!r.ok || typeof r.value !== 'object' || r.value === null) return {};
  const out: Record<string, readonly string[]> = {};
  for (const [name, models] of Object.entries(r.value as Record<string, unknown>)) {
    if (Array.isArray(models)) {
      out[name] = models.filter((m): m is string => typeof m === 'string');
    }
  }
  return out;
}

export function useMainModels(): MainModelsState {
  const [providers, setProviders] = useState<readonly ProviderOption[]>([]);
  const [primary, setPrimaryState] = useState<ModelRef>();
  const [fallbacks, setFallbacksState] = useState<readonly ModelRef[]>([]);
  const [envKeys, setEnvKeys] = useState<readonly EnvRow[]>([]);
  const [envReload, setEnvReload] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [note, setNote] = useState<string>();

  // Stored key rows — which slots exist per env var (masked only, no values).
  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    void deaconRequest('env.list', {}).then((r) => {
      if (!alive || !r.ok) return;
      const v = (r.value ?? {}) as Record<string, unknown>;
      const list = Array.isArray(v.keys) ? v.keys : [];
      setEnvKeys(
        list.flatMap((row) => {
          const o = row as Record<string, unknown>;
          return typeof o.name === 'string'
            ? [{ name: o.name, set: o.set === true, masked: typeof o.masked === 'string' ? o.masked : undefined }]
            : [];
        }),
      );
    });
    return () => {
      alive = false;
    };
  }, [envReload]);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    void deaconRequest('config.get', {}).then(async (r) => {
      if (!alive) return;
      if (!r.ok) {
        setError(r.error.message);
        setLoading(false);
        return;
      }
      const cfg = r.value as Record<string, unknown>;
      const map = (cfg.providers ?? {}) as Record<string, { models?: unknown; api_key_env?: unknown }>;
      // Effective per-provider catalog (deacon `providers.models`): a provider's
      // own `models:` wins; an empty list falls back to its KIND's curated
      // defaults so the dropdown is never blank. An older deacon lacks this op
      // (method-not-found) → we keep the config-listed models only, no error.
      const catalog = await fetchCatalog();
      if (!alive) return;
      setProviders(
        Object.entries(map).map(([name, spec]) => {
          const configured = Array.isArray(spec?.models)
            ? spec.models.filter((m): m is string => typeof m === 'string')
            : [];
          const fromCatalog = catalog[name];
          return {
            name,
            models:
              configured.length > 0 || fromCatalog === undefined
                ? configured
                : fromCatalog,
            keyEnv: typeof spec?.api_key_env === 'string' ? spec.api_key_env : undefined,
          };
        }),
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

  const keySlotsFor = useCallback(
    (provider: string): readonly KeySlot[] => {
      const base = providers.find((p) => p.name === provider)?.keyEnv;
      if (base === undefined) return [];
      const slots: KeySlot[] = [{ slot: 1, masked: envKeys.find((k) => k.name === base)?.masked }];
      for (const k of envKeys) {
        const m = new RegExp(`^${base}_(\\d+)$`).exec(k.name);
        if (m !== null && k.set) slots.push({ slot: Number(m[1]), masked: k.masked });
      }
      return slots.sort((a, b) => a.slot - b.slot);
    },
    [providers, envKeys],
  );

  const activateKey = useCallback((provider: string, slot: number) => {
    const base = providers.find((p) => p.name === provider)?.keyEnv;
    if (base === undefined) return;
    void deaconRequest('env.activate', { name: base, slot }).then((r) => {
      if (!r.ok) setError(r.error.message);
      else setNote((r.value as { note?: string }).note);
      setEnvReload((n) => n + 1); // masks moved between slots — resync
    });
  }, [providers]);

  return {
    providers,
    primary,
    fallbacks,
    loading,
    error,
    note,
    setPrimary,
    setFallbacks,
    keySlotsFor,
    activateKey,
  };
}
