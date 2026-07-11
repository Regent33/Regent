// Voice-server lifecycle seam: probe :8000/health, spawn via the Rust
// `voice_spawn` command when down, poll until it answers. Mirrors `regent
// call`'s reuse semantics — an already-running server is used as-is (with the
// documented stale-binary caveat).
import { invoke } from '@tauri-apps/api/core';
import { type Failure, type Result, err, failure, ok } from '@/shared/kernel/result';
import { isTauri } from '@/shared/infrastructure/rpc/client';

export const SPEECH_URL = import.meta.env.VITE_SPEECH_URL || 'http://localhost:8000';

const PROBE_TIMEOUT_MS = 800;
const BOOT_POLL_MS = 500;
const BOOT_DEADLINE_MS = 30_000;

async function healthy(): Promise<boolean> {
  try {
    const res = await fetch(`${SPEECH_URL}/health`, {
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
    return res.ok;
  } catch {
    return false;
  }
}

/** Make sure a voice server is answering on :8000, spawning one if needed. */
export async function ensureVoiceServer(): Promise<Result<void, Failure>> {
  // Best-effort, before the mic opens: stop Windows from ducking every other
  // app's audio for the duration of the call (Sound → Communications policy).
  if (isTauri()) void invoke('call_ducking_off').catch(() => undefined);
  if (await healthy()) return ok(undefined);
  if (!isTauri()) {
    return err(failure('no-shell', 'voice server is down and only the desktop shell can start it'));
  }
  try {
    await invoke('voice_spawn');
  } catch (cause) {
    return err(failure('voice-spawn', String(cause), cause));
  }
  const deadline = Date.now() + BOOT_DEADLINE_MS;
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, BOOT_POLL_MS));
    if (await healthy()) return ok(undefined);
  }
  return err(failure('voice-boot', 'voice server did not come up within 30s'));
}
