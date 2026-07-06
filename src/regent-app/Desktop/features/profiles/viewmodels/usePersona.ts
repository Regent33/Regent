'use client';
// The SOUL profile editor — persona.get {key:'soul'} / persona.set
// (admin_ops.rs::persona_get/persona_set) return/accept {key, content}.
// `dirty` is derived by comparing the live textarea value against the last
// value the deacon confirmed saved.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

const PERSONA_KEY = 'soul';

export interface PersonaState {
  readonly content: string;
  readonly dirty: boolean;
  readonly loading: boolean;
  readonly saving: boolean;
  readonly error?: string;
  readonly setContent: (content: string) => void;
  readonly save: () => void;
}

export function usePersona(): PersonaState {
  const [content, setContentState] = useState('');
  const [savedContent, setSavedContent] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    void deaconRequest('persona.get', { key: PERSONA_KEY }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setError(result.error.message);
        setLoading(false);
        return;
      }
      const value = result.value as Record<string, unknown>;
      const loaded = typeof value.content === 'string' ? value.content : '';
      setContentState(loaded);
      setSavedContent(loaded);
      setError(undefined);
      setLoading(false);
    });
    return () => {
      alive = false;
    };
  }, []);

  const save = useCallback(() => {
    setSaving(true);
    void deaconRequest('persona.set', { key: PERSONA_KEY, content }).then((result) => {
      setSaving(false);
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      setSavedContent(content);
      setError(undefined);
    });
  }, [content]);

  return {
    content,
    dirty: content !== savedContent,
    loading,
    saving,
    error,
    setContent: setContentState,
    save,
  };
}
