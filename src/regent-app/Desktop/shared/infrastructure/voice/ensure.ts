// Voice-server lifecycle seam: probe :8000/health, spawn via the Rust
// `voice_spawn` command when down, poll until it answers. Mirrors `regent
// call`'s reuse semantics — an already-running server is used as-is (with the
// documented stale-binary caveat).
import { invoke } from '@tauri-apps/api/core';
import { type Failure, type Result, err, failure, ok } from '@/shared/kernel/result';
import { isTauri } from '@/shared/infrastructure/rpc/client';
import {
  isCompatibleVoiceHealth,
  voiceLanguageHint,
} from '@/shared/infrastructure/voice/protocol';

export const SPEECH_URL = import.meta.env.VITE_SPEECH_URL || 'http://localhost:8000';

const PROBE_TIMEOUT_MS = 800;
const BOOT_POLL_MS = 500;
const BOOT_DEADLINE_MS = 30_000;

type Probe = 'current' | 'stale' | 'down';
let ensureInFlight: Promise<Result<void, Failure>> | null = null;

async function probe(): Promise<Probe> {
  try {
    const res = await fetch(`${SPEECH_URL}/health`, {
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
    if (!res.ok) return 'down';
    return isCompatibleVoiceHealth(await res.json()) ? 'current' : 'stale';
  } catch {
    return 'down';
  }
}

/** Make sure a voice server is answering on :8000, spawning one if needed, and
 * — at a real call start — turn off Windows comms-ducking first. */
export function ensureVoiceServer(): Promise<Result<void, Failure>> {
  // Best-effort, before the mic opens: stop Windows from ducking every other
  // app's audio for the duration of the call (Sound → Communications policy).
  if (isTauri()) void invoke('call_ducking_off').catch(() => undefined);
  return bringUp();
}

/** Warm the ASR + TTS engines at app launch so the FIRST Butler call / dictation
 * is instant instead of paying model-load + warmup on first use. Same
 * probe-and-spawn as ensureVoiceServer, but WITHOUT the ducking registry write —
 * that global Windows preference belongs to real voice use, not app launch. */
export function prewarmVoiceServer(): Promise<Result<void, Failure>> {
  return bringUp();
}

/** Collapse concurrent React effects / mic clients into one probe-and-spawn.
 * Without this, two first-use callers can start two processes that both load
 * the large ONNX models before either one has bound port 8000. */
function bringUp(): Promise<Result<void, Failure>> {
  if (ensureInFlight) return ensureInFlight;
  ensureInFlight = bringUpOnce().finally(() => {
    ensureInFlight = null;
  });
  return ensureInFlight;
}

async function bringUpOnce(): Promise<Result<void, Failure>> {
  const state = await probe();
  if (state === 'current') return ok(undefined);
  if (!isTauri()) {
    return err(
      failure(
        'no-shell',
        state === 'stale'
          ? 'voice server is stale and only the desktop shell can replace it'
          : 'voice server is down and only the desktop shell can start it',
      ),
    );
  }
  try {
    const language = voiceLanguageHint(
      typeof navigator === 'undefined' ? undefined : navigator.language,
    );
    await invoke(state === 'stale' ? 'voice_restart' : 'voice_spawn', { language });
  } catch (cause) {
    return err(failure('voice-spawn', String(cause), cause));
  }
  const deadline = Date.now() + BOOT_DEADLINE_MS;
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, BOOT_POLL_MS));
    if ((await probe()) === 'current') return ok(undefined);
  }
  return err(failure('voice-boot', 'voice server did not come up within 30s'));
}
