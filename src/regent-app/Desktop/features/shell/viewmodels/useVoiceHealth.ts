'use client';
// Polls the voice server's /health so the status bar can show whether the ASR
// + TTS models are loaded and warm. The server only runs while Butler is active,
// so 'down' is the normal resting state; 'warming' means it answered but the
// models aren't ready yet (a cold first turn would be slow).
import { useEffect, useState } from 'react';
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';

export type VoiceHealth = 'down' | 'warming' | 'ready';

// Poll fast while the server is down/warming so the brief (few-second) model
// warmup window is actually caught and the amber pulse shows; slow once it's
// warm and steady. A fixed 3s interval routinely skipped the whole window.
const READY_POLL_MS = 4000;
const BUSY_POLL_MS = 1200;
const PROBE_TIMEOUT_MS = 800;

export function useVoiceHealth(): VoiceHealth {
  const [health, setHealth] = useState<VoiceHealth>('down');
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const poll = async () => {
      let next: VoiceHealth = 'down';
      try {
        const res = await fetch(`${SPEECH_URL}/health`, { signal: AbortSignal.timeout(PROBE_TIMEOUT_MS) });
        if (res.ok) {
          const j = (await res.json()) as { asr?: boolean; tts?: boolean; warm?: boolean };
          next = j.asr && j.tts && j.warm ? 'ready' : 'warming';
        }
      } catch {
        // unreachable → down
      }
      if (!alive) return;
      setHealth(next);
      timer = setTimeout(() => void poll(), next === 'ready' ? READY_POLL_MS : BUSY_POLL_MS);
    };
    void poll();
    return () => {
      alive = false;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, []);
  return health;
}
