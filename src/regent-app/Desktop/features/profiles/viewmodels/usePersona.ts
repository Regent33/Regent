'use client';
// One persona row editor — persona.get/persona.set {key, content}
// (admin_ops.rs::persona_get/persona_set). Default key is 'soul'; the About
// facets pass 'about.<facet>' (the same keys persona_block renders into the
// prompt). `dirty` compares the live value against the last confirmed save.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface PersonaState {
  readonly content: string;
  readonly dirty: boolean;
  readonly loading: boolean;
  readonly saving: boolean;
  readonly error?: string;
  readonly setContent: (content: string) => void;
  readonly save: () => void;
}

export function usePersona(personaKey = 'soul'): PersonaState {
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
    void deaconRequest('persona.get', { key: personaKey }).then((result) => {
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
  }, [personaKey]);

  const save = useCallback(() => {
    setSaving(true);
    void deaconRequest('persona.set', { key: personaKey, content }).then((result) => {
      setSaving(false);
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      setSavedContent(content);
      setError(undefined);
    });
  }, [content, personaKey]);

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
