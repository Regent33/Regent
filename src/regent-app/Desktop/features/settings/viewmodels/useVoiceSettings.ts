'use client';
// Voice settings — voice.status for the live summary, voice.models for the
// picker options (configured + built-in provider names, per
// speech_factory::voice_status/voice_models). voice.set edits config.yaml /
// .env; changes apply on the next deacon/voice-server start, never this
// session — its own `note` field says so, surfaced verbatim.
import { useCallback, useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface VoiceStatus {
  readonly enabled: boolean;
  readonly asrProvider?: string;
  readonly asrModel?: string;
  readonly asrAvailable: boolean;
  readonly ttsProvider?: string;
  readonly ttsModel?: string;
  readonly ttsAvailable: boolean;
}

export interface VoiceModelsState {
  readonly asrBuiltins: readonly string[];
  readonly ttsBuiltins: readonly string[];
}

export interface VoiceSettingsState {
  readonly status?: VoiceStatus;
  readonly models: VoiceModelsState;
  readonly loading: boolean;
  readonly error?: string;
  readonly saving: boolean;
  readonly note?: string;
  readonly setAsrModel: (model: string) => void;
  readonly setTtsModel: (model: string) => void;
  readonly setAsrProvider: (provider: string) => void;
  readonly setTtsProvider: (provider: string) => void;
  readonly setWhisperSize: (size: string) => void;
}

function toStatus(v: Record<string, unknown>): VoiceStatus {
  const asr = (v.asr ?? {}) as Record<string, unknown>;
  const tts = (v.tts ?? {}) as Record<string, unknown>;
  return {
    enabled: v.enabled === true,
    asrProvider: typeof asr.provider === 'string' ? asr.provider : undefined,
    asrModel: typeof asr.model === 'string' ? asr.model : undefined,
    asrAvailable: asr.available === true,
    ttsProvider: typeof tts.provider === 'string' ? tts.provider : undefined,
    ttsModel: typeof tts.model === 'string' ? tts.model : undefined,
    ttsAvailable: tts.available === true,
  };
}

function toStringArray(v: unknown): readonly string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
}

export function useVoiceSettings(): VoiceSettingsState {
  const [status, setStatus] = useState<VoiceStatus>();
  const [models, setModels] = useState<VoiceModelsState>({ asrBuiltins: [], ttsBuiltins: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [note, setNote] = useState<string>();
  const [reload, setReload] = useState(0);

  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    void Promise.all([deaconRequest('voice.status', {}), deaconRequest('voice.models', {})]).then(
      ([statusResult, modelsResult]) => {
        if (!alive) return;
        if (!statusResult.ok) {
          setError(statusResult.error.message);
          setLoading(false);
          return;
        }
        setStatus(toStatus(statusResult.value as Record<string, unknown>));
        if (modelsResult.ok) {
          const v = modelsResult.value as Record<string, unknown>;
          const asr = (v.asr ?? {}) as Record<string, unknown>;
          const tts = (v.tts ?? {}) as Record<string, unknown>;
          setModels({
            asrBuiltins: toStringArray(asr.builtins),
            ttsBuiltins: toStringArray(tts.builtins),
          });
        }
        setError(undefined);
        setLoading(false);
      },
    );
    return () => {
      alive = false;
    };
  }, [reload]);

  const setField = useCallback((params: Record<string, unknown>) => {
    setSaving(true);
    setNote(undefined);
    void deaconRequest('voice.set', params).then((result) => {
      setSaving(false);
      if (!result.ok) {
        setError(result.error.message);
        return;
      }
      const value = result.value as Record<string, unknown>;
      setNote(typeof value.note === 'string' ? value.note : undefined);
      setError(undefined);
      setReload((n) => n + 1);
    });
  }, []);

  const setAsrModel = useCallback((model: string) => setField({ asr_model: model }), [setField]);
  const setTtsModel = useCallback((model: string) => setField({ tts_model: model }), [setField]);
  // Providers are a separate config key from models (speech.<kind>.provider) —
  // the picker lists PROVIDERS, so it must never write into the model field.
  const setAsrProvider = useCallback(
    (provider: string) => setField({ asr_provider: provider }),
    [setField],
  );
  const setTtsProvider = useCallback(
    (provider: string) => setField({ tts_provider: provider }),
    [setField],
  );
  const setWhisperSize = useCallback((size: string) => setField({ whisper_size: size }), [setField]);

  return {
    status,
    models,
    loading,
    error,
    saving,
    note,
    setAsrModel,
    setTtsModel,
    setAsrProvider,
    setTtsProvider,
    setWhisperSize,
  };
}
