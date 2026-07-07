'use client';
// Status-bar model menu: `model.list` rows fetched lazily (only once the
// panel opens), `model.set` on pick. The backend's `note` from model.set
// shows for a few seconds then clears — never a persistent banner.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface ModelOption {
  readonly id: string;
  readonly label: string;
  readonly current: boolean;
}

export interface ModelMenuState {
  readonly open: boolean;
  readonly toggle: () => void;
  readonly close: () => void;
  readonly items: readonly ModelOption[];
  readonly loading: boolean;
  readonly error?: string;
  readonly note?: string;
  readonly select: (id: string) => void;
}

const NOTE_MS = 4000;

function toOption(value: unknown): ModelOption | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const v = value as Record<string, unknown>;
  const id = typeof v.id === 'string' ? v.id : undefined;
  if (id === undefined) return undefined;
  return { id, label: typeof v.display_name === 'string' ? v.display_name : id, current: v.current === true };
}

/** Called by the status bar once a `model.set` succeeds, so the base model
 * label (from useStatus) reflects the change without its own refetch. */
export function useModelMenu(onChanged: () => void): ModelMenuState {
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<readonly ModelOption[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [note, setNote] = useState<string>();

  useEffect(() => {
    if (!open || !isTauri()) return;
    let alive = true;
    setLoading(true);
    void deaconRequest('model.list', {}).then((result) => {
      if (!alive) return;
      setLoading(false);
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      const list = Array.isArray(result.value) ? result.value : [];
      setItems(list.map(toOption).filter((o): o is ModelOption => o !== undefined));
      setError(undefined);
    });
    return () => {
      alive = false;
    };
  }, [open]);

  useEffect(() => {
    if (note === undefined) return;
    const timer = setTimeout(() => setNote(undefined), NOTE_MS);
    return () => clearTimeout(timer);
  }, [note]);

  const select = useCallback(
    (id: string) => {
      void deaconRequest<{ model?: string; note?: string }>('model.set', { model: id }).then((result) => {
        if (!result.ok) {
          setError(result.error.message);
          return;
        }
        setItems((prev) => prev.map((o) => ({ ...o, current: o.id === id })));
        setNote(result.value.note);
        onChanged();
        setOpen(false);
      });
    },
    [onChanged],
  );

  return {
    open,
    toggle: () => setOpen((o) => !o),
    close: () => setOpen(false),
    items,
    loading,
    error,
    note,
    select,
  };
}
