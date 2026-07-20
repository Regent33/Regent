// Non-React plumbing for the composer's push-to-talk mic: browser
// SpeechRecognition typings, MediaRecorder helpers, and the transcription
// call to the local voice server. WAV encoding lives in wavEncode.ts.
import { ensureVoiceServer, SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { blobToWav } from '@/features/chat/data/wavEncode';

export const MAX_RECORDING_MS = 60_000;
export const PREVIEW_TRANSCRIBE_MS = 3_500;
const TRANSCRIBE_TIMEOUT_MS = 120_000;
const MIME_TYPES = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg;codecs=opus', 'audio/mp4'];

export interface ActiveRecording {
  recorder: MediaRecorder;
  stream: MediaStream;
  chunks: Blob[];
  mimeType: string;
  stopped: Promise<Blob>;
  // Warmed in the background at record start; awaited before transcribe (stop).
  serverReady: ReturnType<typeof ensureVoiceServer>;
  recognition?: BrowserSpeechRecognition;
  previewTimer?: number;
  previewing: boolean;
}

interface BrowserSpeechRecognitionAlternative {
  readonly transcript: string;
}

interface BrowserSpeechRecognitionResult {
  readonly isFinal: boolean;
  readonly length: number;
  readonly [index: number]: BrowserSpeechRecognitionAlternative | undefined;
}

interface BrowserSpeechRecognitionResultList {
  readonly length: number;
  readonly [index: number]: BrowserSpeechRecognitionResult | undefined;
}

export interface BrowserSpeechRecognitionEvent extends Event {
  readonly resultIndex: number;
  readonly results: BrowserSpeechRecognitionResultList;
}

export interface BrowserSpeechRecognitionErrorEvent extends Event {
  readonly error?: string;
  readonly message?: string;
}

export interface BrowserSpeechRecognition extends EventTarget {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  onresult: ((event: BrowserSpeechRecognitionEvent) => void) | null;
  onerror: ((event: BrowserSpeechRecognitionErrorEvent) => void) | null;
  onend: (() => void) | null;
  start: () => void;
  stop: () => void;
  abort: () => void;
}

type BrowserSpeechRecognitionConstructor = new () => BrowserSpeechRecognition;

export function supportedMimeType(): string | undefined {
  return MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type));
}

export function speechRecognitionCtor(): BrowserSpeechRecognitionConstructor | undefined {
  if (typeof window === 'undefined') return undefined;
  const w = window as Window & {
    SpeechRecognition?: BrowserSpeechRecognitionConstructor;
    webkitSpeechRecognition?: BrowserSpeechRecognitionConstructor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition;
}

async function errorMessage(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { error?: unknown };
    if (typeof body.error === 'string' && body.error.trim() !== '') return body.error;
  } catch {
    // Fall through to text/status below.
  }
  const text = await res.text().catch(() => '');
  return text.trim() || `Speech transcription failed (${res.status})`;
}

export async function transcribe(blob: Blob): Promise<string> {
  const wav = await blobToWav(blob);
  const form = new FormData();
  form.append('file', new Blob([wav], { type: 'audio/wav' }), 'speech.wav');
  form.append('model', 'local');

  const res = await fetch(`${SPEECH_URL}/v1/audio/transcriptions`, {
    method: 'POST',
    body: form,
    signal: AbortSignal.timeout(TRANSCRIBE_TIMEOUT_MS),
  });
  if (!res.ok) throw new Error(await errorMessage(res));
  const data = (await res.json()) as { text?: unknown };
  return typeof data.text === 'string' ? data.text.trim() : '';
}

/**
 * Browser live-caption recognition, wired for continuous preview. `onPreview`
 * receives the accumulated transcript; `schedulePreview` arms the
 * local-transcribe fallback when the engine dies and won't restart.
 * Returns undefined where SpeechRecognition doesn't exist (fallback only).
 */
export function createLiveRecognition(
  active: () => ActiveRecording | undefined,
  onPreview: (text: string) => void,
  schedulePreview: (a: ActiveRecording) => void,
): BrowserSpeechRecognition | undefined {
  const Recognition = speechRecognitionCtor();
  if (Recognition === undefined) return undefined;
  const recognition = new Recognition();
  recognition.continuous = true;
  recognition.interimResults = true;
  recognition.lang = 'en-US';
  recognition.onresult = (event) => {
    let text = '';
    for (let i = 0; i < event.results.length; i++) text += event.results[i]?.[0]?.transcript ?? '';
    onPreview(text.trim());
  };
  recognition.onerror = (event) => {
    if (event.error !== 'no-speech') {
      console.debug(`[chat-mic] live speech recognition skipped: ${event.error ?? event.message ?? 'unknown'}`);
    }
  };
  recognition.onend = () => {
    const a = active();
    if (a?.recognition !== recognition || a.recorder.state !== 'recording') return;
    try {
      recognition.start();
    } catch {
      a.recognition = undefined;
      if (a.previewTimer === undefined) schedulePreview(a);
    }
  };
  return recognition;
}

export function stopStream(stream: MediaStream): void {
  for (const track of stream.getTracks()) track.stop();
}

export function recordedBlob(active: ActiveRecording): Blob {
  return new Blob(active.chunks, { type: active.mimeType });
}

export function stopRecognition(active: ActiveRecording): void {
  const recognition = active.recognition;
  if (recognition === undefined) return;
  recognition.onend = null;
  recognition.onresult = null;
  recognition.onerror = null;
  try {
    recognition.stop();
  } catch {
    try {
      recognition.abort();
    } catch {
      // Already stopped.
    }
  }
  active.recognition = undefined;
}
