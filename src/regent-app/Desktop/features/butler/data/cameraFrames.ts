// Camera → agent vision — port of regent-web/hooks/localCall.ts
// startCameraFrames: while the call runs and the stream has a video track,
// POST a small JPEG every 2.5s to /call/frame; the voice server writes it to
// $REGENT_HOME/voice/camera-frame.jpg where the agent's camera_capture tool
// reads it while fresh (≤10s). No video track → no-op.
//
// Grabs frames via ImageCapture (built for exactly this) instead of a hidden
// <video> + canvas.drawImage: drawing a LIVE video element to canvas forces a
// synchronous GPU→CPU readback on the main thread — the same thread running
// the call's ScriptProcessorNode VAD loop — and stalling it there was landing
// as "the mic randomly goes deaf for a beat" / slow-feeling turns right after
// this camera feature shipped. grabFrame() decodes off that path.
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { fetchCallToken } from '@/features/butler/data/speechClient';

const FRAME_INTERVAL_MS = 2500;

export function startCameraFrames(stream: MediaStream): () => void {
  const [track] = stream.getVideoTracks();
  if (!track) return () => {};
  // Guarded: this runs before the call reaches 'listening' (see
  // useButlerCall) — an unguarded throw here (an unsupported ImageCapture,
  // or a track not yet in a grabbable state) would abort that whole setup
  // and strand the call, mic included. Degrade to no frames, never to no call.
  let capture: ImageCapture;
  try {
    capture = new ImageCapture(track);
  } catch (e) {
    console.warn('[butler] camera capture unavailable — no frames this call', e);
    return () => {};
  }
  const canvas = document.createElement('canvas');
  let busy = false; // never queue a second grab while one is still encoding
  const timer = setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      const bitmap = await capture.grabFrame();
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      canvas.getContext('2d')?.drawImage(bitmap, 0, 0);
      bitmap.close();
      const token = await fetchCallToken();
      canvas.toBlob(
        (blob) => {
          if (blob) {
            void fetch(`${SPEECH_URL}/call/frame`, {
              method: 'POST',
              body: blob,
              headers: { 'x-call-token': token },
            }).catch(() => {});
          }
          busy = false;
        },
        'image/jpeg',
        0.7,
      );
    } catch {
      busy = false; // track momentarily not readable — try again next tick
    }
  }, FRAME_INTERVAL_MS);
  return () => clearInterval(timer);
}
