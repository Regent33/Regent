'use client';
// Main-model settings, Hermes "Main model" layout. Reads config.get for the
// current model.{provider,default,base_url} and the configured providers map
// (name -> {kind, base_url, models}). Apply writes three paths through
// config.set (the safe, whole-file-validating path):
//   model.provider = <kind>   model.default = <model>   model.base_url = <url|null>
// A configured provider carries a known-good base_url + kind; a bare built-in
// kind sets base_url to null so the kind's own default applies. config.set's
// note (or its rejection reason) surfaces verbatim.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

// The provider kinds config.set accepts for model.provider (task spec).
export const PROVIDER_KINDS = [
  'anthropic',
  'openai',
  'openrouter',
  'groq',
  'deepseek',
  'together',
  'ollama',
  'mistral',
  'xai',
  'gemini',
  'moonshot',
  'zhipu',
  'dashscope',
  'fireworks',
  'cerebras',
  'perplexity',
  'minimax',
] as const;

export interface ConfiguredProvider {
  readonly name: string;
  readonly kind: string;
  readonly baseUrl?: string;
  readonly models: readonly string[];
}

export interface ProviderOption {
  readonly value: string; // "cfg:<name>" or "kind:<kind>"
  readonly label: string;
}

export interface ModelConfigState {
  readonly loading: boolean;
  readonly error?: string;
  readonly note?: string;
  readonly applying: boolean;
  readonly currentProvider?: string; // kind
  readonly currentModel?: string;
  readonly currentValue: string; // matching provider-select value
  readonly configured: readonly ConfiguredProvider[];
  readonly providerOptions: readonly ProviderOption[];
  readonly apply: (providerValue: string, model: string) => void;
}

function parseConfigured(raw: unknown): ConfiguredProvider[] {
  if (raw === null || typeof raw !== 'object') return [];
  return Object.entries(raw as Record<string, unknown>).map(([name, value]) => {
    const v = (value ?? {}) as Record<string, unknown>;
    return {
      name,
      kind: typeof v.kind === 'string' ? v.kind : name,
      baseUrl: typeof v.base_url === 'string' ? v.base_url : undefined,
      models: Array.isArray(v.models) ? v.models.filter((m): m is string => typeof m === 'string') : [],
    };
  });
}

// Which select option represents the applied model: a configured provider that
// matches both kind and base_url, else the bare kind.
function currentSelectValue(
  configured: readonly ConfiguredProvider[],
  provider?: string,
  baseUrl?: string,
): string {
  if (provider === undefined) return '';
  const match = configured.find(
    (p) => p.kind === provider && (p.baseUrl ?? null) === (baseUrl ?? null),
  );
  return match ? `cfg:${match.name}` : `kind:${provider}`;
}

export function useModelConfig(): ModelConfigState {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [note, setNote] = useState<string>();
  const [applying, setApplying] = useState(false);
  const [currentProvider, setCurrentProvider] = useState<string>();
  const [currentModel, setCurrentModel] = useState<string>();
  const [currentBaseUrl, setCurrentBaseUrl] = useState<string>();
  const [configured, setConfigured] = useState<readonly ConfiguredProvider[]>([]);

  const load = useCallback(async () => {
    const result = await deaconRequest('config.get', {});
    if (!result.ok) {
      setError(result.error.message);
      setLoading(false);
      return;
    }
    const cfg = (result.value ?? {}) as Record<string, unknown>;
    const model = (cfg.model ?? {}) as Record<string, unknown>;
    setCurrentProvider(typeof model.provider === 'string' ? model.provider : undefined);
    setCurrentModel(typeof model.default === 'string' ? model.default : undefined);
    setCurrentBaseUrl(typeof model.base_url === 'string' ? model.base_url : undefined);
    setConfigured(parseConfigured(cfg.providers));
    setError(undefined);
    setLoading(false);
  }, []);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    void load();
  }, [load]);

  // Configured providers first (each carries a known-good base_url + kind),
  // then every built-in kind (a bare kind clears base_url to its own default).
  const providerOptions: ProviderOption[] = [
    ...configured.map((p) => ({ value: `cfg:${p.name}`, label: p.name })),
    ...PROVIDER_KINDS.map((kind) => ({ value: `kind:${kind}`, label: kind })),
  ];

  const apply = useCallback(
    (providerValue: string, model: string) => {
      const trimmed = model.trim();
      if (providerValue === '' || trimmed === '') return;

      let kind: string;
      let baseUrl: string | null;
      if (providerValue.startsWith('cfg:')) {
        const p = configured.find((c) => `cfg:${c.name}` === providerValue);
        if (p === undefined) return;
        kind = p.kind;
        baseUrl = p.baseUrl ?? null;
      } else {
        kind = providerValue.slice('kind:'.length);
        baseUrl = null; // bare kind -> the kind's own default base_url
      }

      setApplying(true);
      setNote(undefined);
      void (async () => {
        const writes: readonly [string, unknown][] = [
          ['model.provider', kind],
          ['model.default', trimmed],
          ['model.base_url', baseUrl],
        ];
        let lastNote: string | undefined;
        for (const [path, value] of writes) {
          const r = await deaconRequest('config.set', { path, value });
          if (!r.ok) {
            setError(r.error.message);
            setApplying(false);
            await load(); // re-sync to the accepted state
            return;
          }
          const v = r.value as Record<string, unknown>;
          if (typeof v.note === 'string') lastNote = v.note;
        }
        setError(undefined);
        setNote(lastNote);
        setApplying(false);
        await load();
      })();
    },
    [configured, load],
  );

  return {
    loading,
    error,
    note,
    applying,
    currentProvider,
    currentModel,
    currentValue: currentSelectValue(configured, currentProvider, currentBaseUrl),
    configured,
    providerOptions,
    apply,
  };
}
