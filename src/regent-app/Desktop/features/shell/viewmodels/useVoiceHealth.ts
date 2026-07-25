'use client';
// Polls the voice server's /health so the status bar can show whether the ASR
// + TTS models are loaded and warm. The server only runs while Butler is active,
// so 'down' is the normal resting state; 'warming' means it answered but the
// models aren't ready yet (a cold first turn would be slow).
import { useEffect, useState } from 'react';
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';

export type VoiceHealth = 'down' | 'warming' | 'ready';

export interface VoiceHealthPayload {
  readonly asr?: boolean;
  readonly tts?: boolean;
  readonly warm?: boolean;
  readonly agent?: string;
}

/** Ready means the complete Butler path, not only warm speech engines: the
 * engines can be warm while the agent deacon is unreachable, which is what made
 * the status bar claim "Voice ready" while Butler Mode sat on "Connecting".
 * `agent` is additive — absent means an older server, so we degrade to the
 * legacy engines-only contract rather than pulse amber forever (ADR-041 §4.3). */
export function classifyVoiceHealth(health: VoiceHealthPayload): VoiceHealth {
  if (!health.asr || !health.tts || !health.warm) return 'warming';
  if (health.agent === undefined || health.agent === 'ready') return 'ready';
  return health.agent === 'warming up' ? 'warming' : 'down';
}

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
        if (res.ok) next = classifyVoiceHealth((await res.json()) as VoiceHealthPayload);
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
