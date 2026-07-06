'use client';
// Model settings — model.get for the active model, model.list for the
// selectable catalog (built-ins + configured providers' models, per
// admin_ops.rs::model_list). Picking a row calls model.set; the deacon's own
// note says it only applies to new sessions, so we surface that verbatim
// instead of pretending the switch is live.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface ModelOption {
  readonly id: string;
  readonly displayName: string;
  readonly current: boolean;
}

export interface ModelSettingsState {
  readonly current?: string;
  readonly options: readonly ModelOption[];
  readonly loading: boolean;
  readonly error?: string;
  readonly saving: boolean;
  readonly note?: string;
  readonly setModel: (id: string) => void;
}

function toOption(value: unknown): ModelOption | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.id === 'string' ? v.id : undefined;
  if (id === undefined) return undefined;
  return {
    id,
    displayName: typeof v.display_name === 'string' ? v.display_name : id,
    current: v.current === true,
  };
}

export function useModelSettings(): ModelSettingsState {
  const [current, setCurrent] = useState<string>();
  const [options, setOptions] = useState<readonly ModelOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [note, setNote] = useState<string>();

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    void Promise.all([deaconRequest('model.get', {}), deaconRequest('model.list', {})]).then(
      ([got, listed]) => {
        if (!alive) return;
        if (!got.ok) {
          setError(got.error.message);
          setLoading(false);
          return;
        }
        const value = got.value as Record<string, unknown>;
        setCurrent(typeof value.model === 'string' ? value.model : undefined);
        if (listed.ok && Array.isArray(listed.value)) {
          setOptions(listed.value.map(toOption).filter((o): o is ModelOption => o !== undefined));
        }
        setError(undefined);
        setLoading(false);
      },
    );
    return () => {
      alive = false;
    };
  }, []);

  const setModel = useCallback((id: string) => {
    setSaving(true);
    setNote(undefined);
    void deaconRequest('model.set', { model: id }).then((result) => {
      setSaving(false);
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      const value = result.value as Record<string, unknown>;
      setCurrent(typeof value.model === 'string' ? value.model : id);
      setNote(typeof value.note === 'string' ? value.note : undefined);
      setOptions((prev) => prev.map((o) => ({ ...o, current: o.id === id })));
      setError(undefined);
    });
  }, []);

  return { current, options, loading, error, saving, note, setModel };
}
