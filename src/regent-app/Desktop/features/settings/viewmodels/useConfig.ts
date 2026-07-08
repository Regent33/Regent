'use client';
// Generic config engine — reads config.get ONCE, then binds dotted paths to
// controls (see ConfigField). Every write goes through config.set, the SAFE
// path: it re-validates the whole file against the real schema and returns the
// reason verbatim on rejection, which we surface unmodified. Writes are
// optimistic on the local copy and re-synced from disk if the deacon rejects.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { getPath, setPath } from './configPath';

export interface ConfigState {
  readonly config?: Record<string, unknown>;
  readonly loading: boolean;
  /** config.get load failure, verbatim. */
  readonly error?: string;
  /** Note from the last successful config.set, verbatim. */
  readonly note?: string;
  /** Reason from the last rejected config.set, verbatim. */
  readonly writeError?: string;
  /** Path currently being written (drives the per-field saving state). */
  readonly savingPath?: string;
  readonly get: (path: string) => unknown;
  readonly set: (path: string, value: unknown) => void;
}

async function fetchConfig(): Promise<Record<string, unknown> | undefined> {
  const result = await deaconRequest('config.get', {});
  return result.ok ? ((result.value ?? {}) as Record<string, unknown>) : undefined;
}

export function useConfig(): ConfigState {
  const [config, setConfig] = useState<Record<string, unknown>>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [note, setNote] = useState<string>();
  const [writeError, setWriteError] = useState<string>();
  const [savingPath, setSavingPath] = useState<string>();

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    void deaconRequest('config.get', {}).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      setConfig((result.value ?? {}) as Record<string, unknown>);
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  const get = useCallback(
    (path: string) => (config === undefined ? undefined : getPath(config, path)),
    [config],
  );

  const set = useCallback((path: string, value: unknown) => {
    setSavingPath(path);
    setNote(undefined);
    setWriteError(undefined);
    setConfig((prev) => (prev === undefined ? prev : setPath(prev, path, value)));
    void deaconRequest('config.set', { path, value }).then((result) => {
      setSavingPath(undefined);
      if (!result.ok) {
        setWriteError(result.error.message);
        // The optimistic value may now diverge from disk — re-sync so the
        // control reflects what the schema actually accepted.
        void fetchConfig().then((fresh) => fresh !== undefined && setConfig(fresh));
        return;
      }
      const v = result.value as Record<string, unknown>;
      setNote(typeof v.note === 'string' ? v.note : undefined);
    });
  }, []);

  return { config, loading, error, note, writeError, savingPath, get, set };
}
