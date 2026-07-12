'use client';
// Polls the voice server's /health so the status bar can show whether the ASR
// + TTS models are loaded and warm. The server only runs while Butler is active,
// so 'down' is the normal resting state; 'warming' means it answered but the
// models aren't ready yet (a cold first turn would be slow).
import { useEffect, useState } from 'react';
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';

export type VoiceHealth = 'down' | 'warming' | 'ready';

const POLL_MS = 3000;
const PROBE_TIMEOUT_MS = 800;

export function useVoiceHealth(): VoiceHealth {
  const [health, setHealth] = useState<VoiceHealth>('down');
  useEffect(() => {
    let alive = true;
    const poll = async () => {
      try {
        const res = await fetch(`${SPEECH_URL}/health`, { signal: AbortSignal.timeout(PROBE_TIMEOUT_MS) });
        if (!res.ok) throw new Error('unhealthy');
        const j = (await res.json()) as { asr?: boolean; tts?: boolean; warm?: boolean };
        if (alive) setHealth(j.asr && j.tts && j.warm ? 'ready' : 'warming');
      } catch {
        if (alive) setHealth('down');
      }
    };
    void poll();
    const id = setInterval(() => void poll(), POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);
  return health;
}
