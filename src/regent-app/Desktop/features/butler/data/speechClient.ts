// Speech-server I/O — port of regent-web/hooks/speechServer.ts: the per-boot
// call token, the 16kHz WAV encoding the server expects, and gapless chunk
// playback through the analyser (which also feeds the particle core).
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';

// The Rust speech server gates /call/turn behind a per-boot token served at
// /call/token — readable only by CORS-granted origins. If this fetch fails on
// a running server, the server was started WITHOUT the desktop's origin grant
// (e.g. by the CLI) — the caller surfaces that as an actionable error.
let callToken: string | null = null;
export async function fetchCallToken(): Promise<string> {
  if (callToken !== null) return callToken;
  try {
    const res = await fetch(`${SPEECH_URL}/call/token`);
    callToken = res.ok ? String(((await res.json()) as { token?: string }).token ?? '') : '';
  } catch {
    callToken = '';
  }
  return callToken;
}

/** Holds the source node currently playing, so a barge-in can stop it. */
export interface Playing {
  src: AudioBufferSourceNode | null;
}

/** Decode a base64 WAV chunk and play it through the analyser + speakers. */
export async function playPcm(
  ctx: AudioContext,
  node: AnalyserNode,
  b64: string,
  signal: AbortSignal,
  playing: Playing,
): Promise<void> {
  if (signal.aborted) return;
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  let audioBuf: AudioBuffer;
  try {
    audioBuf = await ctx.decodeAudioData(bytes.buffer);
  } catch {
    return;
  }
  if (signal.aborted) return;
  const src = ctx.createBufferSource();
  src.buffer = audioBuf;
  src.connect(node); // the core reacts to Regent's voice
  src.connect(ctx.destination); // and you hear it
  playing.src = src;
  await new Promise<void>((resolve) => {
    src.onended = () => {
      src.disconnect(); // don't leave finished nodes on the graph (grows over a call)
      if (playing.src === src) playing.src = null;
      resolve();
    };
    src.start();
  });
}

/** Downsample float frames to 16 kHz mono PCM16 and wrap in a WAV container. */
export function wavBytes(frames: Float32Array[], sampleRate: number): ArrayBuffer {
  const DST = 16000;
  let total = 0;
  for (const f of frames) total += f.length;
  const all = new Float32Array(total);
  let off = 0;
  for (const f of frames) {
    all.set(f, off);
    off += f.length;
  }
  const ratio = sampleRate / DST;
  const len = Math.floor(all.length / ratio);
  const pcm = new Int16Array(len);
  for (let i = 0; i < len; i++) {
    let v = all[Math.floor(i * ratio)] || 0;
    v = Math.max(-1, Math.min(1, v));
    pcm[i] = v < 0 ? v * 32768 : v * 32767;
  }
  const out = new ArrayBuffer(44 + len * 2);
  const dv = new DataView(out);
  const W = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) dv.setUint8(o + i, s.charCodeAt(i));
  };
  W(0, 'RIFF');
  dv.setUint32(4, 36 + len * 2, true);
  W(8, 'WAVE');
  W(12, 'fmt ');
  dv.setUint32(16, 16, true);
  dv.setUint16(20, 1, true);
  dv.setUint16(22, 1, true);
  dv.setUint32(24, DST, true);
  dv.setUint32(28, DST * 2, true);
  dv.setUint16(32, 2, true);
  dv.setUint16(34, 16, true);
  W(36, 'data');
  dv.setUint32(40, len * 2, true);
  for (let i = 0; i < len; i++) dv.setInt16(44 + i * 2, pcm[i], true);
  return out;
}
