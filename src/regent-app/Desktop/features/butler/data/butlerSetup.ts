// Call-open plumbing: acquire the mic (camera optional) and poll the voice
// server's /health while its engines warm up.
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { micConstraint } from '@/shared/infrastructure/mic';
import { cameraConstraint } from '@/shared/infrastructure/camera';
import { type ButlerState, isWarmingError } from '@/features/butler/domain/phase';

type SetButlerState = (update: (s: ButlerState) => ButlerState) => void;

export async function openButlerStream(): Promise<MediaStream> {
  try {
    return await navigator.mediaDevices.getUserMedia({
      audio: micConstraint(),
      video: cameraConstraint(),
    });
  } catch {
    // Camera is optional; a denied/absent camera must not kill voice capture.
    return navigator.mediaDevices.getUserMedia({ audio: micConstraint() });
  }
}

/**
 * First run, the server answers /health immediately but loads the
 * whisper/kokoro engines in the background — a turn spoken before they're in
 * returns a "still loading" line that used to sit on screen FOREVER (nothing
 * re-checked warmth; exiting and reopening "fixed" it). Poll /health while
 * cold: show its live note (download MBs), and clear the warming state the
 * moment `warm` flips true. Returns the cleanup that stops the poll.
 */
export function startWarmPoll(isCancelled: () => boolean, setState: SetButlerState): () => void {
  const warmPoll = window.setInterval(() => {
    void (async () => {
      try {
        const res = await fetch(`${SPEECH_URL}/health`, {
          signal: AbortSignal.timeout(1500),
        });
        if (!res.ok || isCancelled()) return;
        const h = (await res.json()) as { warm?: boolean; note?: string };
        if (isCancelled()) return;
        if (h.warm) {
          window.clearInterval(warmPoll);
          setState((s) => (isWarmingError(s.error) ? { ...s, error: null } : s));
        } else {
          // Only occupy the error slot when it's free or already ours —
          // a mic-denied or audio-stuck message must never be overwritten.
          setState((s) =>
            s.error === null || isWarmingError(s.error)
              ? { ...s, error: h.note || 'voice engines warming up…' }
              : s,
          );
        }
      } catch {
        // transient probe failure — keep polling
      }
    })();
  }, 2000);
  return () => window.clearInterval(warmPoll);
}
