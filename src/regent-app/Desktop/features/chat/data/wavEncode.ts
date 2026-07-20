// Recorded-audio → 16 kHz mono WAV for the transcription endpoint.
export const TARGET_SAMPLE_RATE = 16_000;

export function audioContextCtor(): typeof AudioContext | undefined {
  if (typeof window === 'undefined') return undefined;
  return window.AudioContext ?? (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
}

export function encodeWav(audio: AudioBuffer): ArrayBuffer {
  const len = Math.floor(audio.duration * TARGET_SAMPLE_RATE);
  const pcm = new Int16Array(len);
  const ratio = audio.sampleRate / TARGET_SAMPLE_RATE;

  for (let i = 0; i < len; i++) {
    const sourceIndex = Math.min(audio.length - 1, Math.floor(i * ratio));
    let mixed = 0;
    for (let channel = 0; channel < audio.numberOfChannels; channel++) {
      mixed += audio.getChannelData(channel)[sourceIndex] ?? 0;
    }
    const sample = Math.max(-1, Math.min(1, mixed / audio.numberOfChannels));
    pcm[i] = sample < 0 ? sample * 32768 : sample * 32767;
  }

  const out = new ArrayBuffer(44 + pcm.length * 2);
  const view = new DataView(out);
  const write = (offset: number, value: string) => {
    for (let i = 0; i < value.length; i++) view.setUint8(offset + i, value.charCodeAt(i));
  };

  write(0, 'RIFF');
  view.setUint32(4, 36 + pcm.length * 2, true);
  write(8, 'WAVE');
  write(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, TARGET_SAMPLE_RATE, true);
  view.setUint32(28, TARGET_SAMPLE_RATE * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  write(36, 'data');
  view.setUint32(40, pcm.length * 2, true);
  for (let i = 0; i < pcm.length; i++) view.setInt16(44 + i * 2, pcm[i], true);

  return out;
}

export async function blobToWav(blob: Blob): Promise<ArrayBuffer> {
  const Ctor = audioContextCtor();
  if (Ctor === undefined) throw new Error('Audio decoding is not available in this webview.');
  const ctx = new Ctor();
  try {
    const bytes = await blob.arrayBuffer();
    const audio = await ctx.decodeAudioData(bytes.slice(0));
    return encodeWav(audio);
  } finally {
    await ctx.close().catch(() => undefined);
  }
}
