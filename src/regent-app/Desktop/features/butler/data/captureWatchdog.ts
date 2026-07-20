// Capture-liveness watchdog. `currentTime` can advance even when
// ScriptProcessor callbacks have silently stopped, so watch the actual
// mic-frame heartbeat and rebuild the whole capture graph (including the OS
// stream) if it dies. Hidden pages are ignored so normal WebView throttling
// never creates a restart loop.
import { captureNeedsRestart } from '@/features/butler/domain/captureLiveness';

export interface WatchdogDeps {
  ctx: AudioContext;
  isCancelled: () => boolean;
  lastAudioFrameAt: () => number;
  audioTrack: () => MediaStreamTrack | null;
  requestRestart: () => void;
}

/** Returns the cleanup that stops the watchdog interval. */
export function startCaptureWatchdog(deps: WatchdogDeps): () => void {
  let restartQueued = false;
  const timer = window.setInterval(() => {
    if (deps.isCancelled() || restartQueued) return;
    const track = deps.audioTrack();
    const trackEnded = track?.readyState === 'ended';
    // A suspended context needs a user-activation resume, not a new stream;
    // rebuilding it would only churn every five seconds. The pointer/key
    // listener in useButlerCall owns that path. A physically ended track
    // still needs replacement immediately.
    if (!trackEnded && deps.ctx.state !== 'running') {
      void deps.ctx.resume();
      return;
    }
    const restart = captureNeedsRestart(
      deps.lastAudioFrameAt(),
      performance.now(),
      trackEnded,
      document.visibilityState === 'visible',
    );
    if (!restart) return;
    restartQueued = true;
    deps.requestRestart();
  }, 1_000);
  return () => window.clearInterval(timer);
}
