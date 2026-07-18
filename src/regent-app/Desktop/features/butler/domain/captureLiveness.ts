// Pure capture-heartbeat policy. The Web Audio clock can keep advancing while
// a deprecated ScriptProcessorNode has stopped delivering mic frames, so
// `AudioContext.currentTime` alone cannot detect Butler's frozen listener.

export const CAPTURE_STALE_MS = 5_000;

export function captureNeedsRestart(
  lastFrameAt: number,
  now: number,
  trackEnded: boolean,
  pageVisible: boolean,
): boolean {
  if (!pageVisible) return false;
  return trackEnded || now - lastFrameAt >= CAPTURE_STALE_MS;
}
