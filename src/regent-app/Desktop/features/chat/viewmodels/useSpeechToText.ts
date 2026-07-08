'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { micConstraint } from '@/shared/infrastructure/mic';
import { openMicPrivacySettings } from '@/shared/infrastructure/opener';
import { ensureVoiceServer, SPEECH_URL } from '@/shared/infrastructure/voice/ensure';

type SpeechState = 'idle' | 'starting' | 'recording' | 'transcribing';

export interface SpeechToTextState {
  readonly state: SpeechState;
  readonly error?: string;
  readonly supported: boolean;
  readonly toggle: () => void;
  readonly clearError: () => void;
}

export interface SpeechToTextCallbacks {
  readonly onStart: () => void;
  readonly onPreview: (text: string) => void;
  readonly onFinal: (text: string) => void;
  readonly onCancel: () => void;
}

interface ActiveRecording {
  recorder: MediaRecorder;
  stream: MediaStream;
  chunks: Blob[];
  mimeType: string;
  stopped: Promise<Blob>;
  recognition?: BrowserSpeechRecognition;
  previewTimer?: number;
  previewing: boolean;
}

const MAX_RECORDING_MS = 60_000;
const PREVIEW_TRANSCRIBE_MS = 3_500;
const TRANSCRIBE_TIMEOUT_MS = 120_000;
const TARGET_SAMPLE_RATE = 16_000;
const MIME_TYPES = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg;codecs=opus', 'audio/mp4'];

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

interface BrowserSpeechRecognitionEvent extends Event {
  readonly resultIndex: number;
  readonly results: BrowserSpeechRecognitionResultList;
}

interface BrowserSpeechRecognitionErrorEvent extends Event {
  readonly error?: string;
  readonly message?: string;
}

interface BrowserSpeechRecognition extends EventTarget {
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

function supportedMimeType(): string | undefined {
  return MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type));
}

function audioContextCtor(): typeof AudioContext | undefined {
  if (typeof window === 'undefined') return undefined;
  return window.AudioContext ?? (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
}

function speechRecognitionCtor(): BrowserSpeechRecognitionConstructor | undefined {
  if (typeof window === 'undefined') return undefined;
  const w = window as Window & {
    SpeechRecognition?: BrowserSpeechRecognitionConstructor;
    webkitSpeechRecognition?: BrowserSpeechRecognitionConstructor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition;
}

function encodeWav(audio: AudioBuffer): ArrayBuffer {
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

async function blobToWav(blob: Blob): Promise<ArrayBuffer> {
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

async function transcribe(blob: Blob): Promise<string> {
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

function stopStream(stream: MediaStream): void {
  for (const track of stream.getTracks()) track.stop();
}

function recordedBlob(active: ActiveRecording): Blob {
  return new Blob(active.chunks, { type: active.mimeType });
}

function stopRecognition(active: ActiveRecording): void {
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

export function useSpeechToText(callbacks: SpeechToTextCallbacks): SpeechToTextState {
  const [state, setState] = useState<SpeechState>('idle');
  const [error, setError] = useState<string>();
  const activeRef = useRef<ActiveRecording | undefined>(undefined);
  const timeoutRef = useRef<number | undefined>(undefined);
  const callbacksRef = useRef(callbacks);
  const liveTextRef = useRef('');
  const supported =
    typeof window !== 'undefined' &&
    typeof navigator !== 'undefined' &&
    navigator.mediaDevices !== undefined &&
    typeof MediaRecorder !== 'undefined' &&
    audioContextCtor() !== undefined;

  useEffect(() => {
    callbacksRef.current = callbacks;
  }, [callbacks]);

  const clearActive = useCallback(() => {
    if (timeoutRef.current !== undefined) {
      window.clearTimeout(timeoutRef.current);
      timeoutRef.current = undefined;
    }
    const active = activeRef.current;
    activeRef.current = undefined;
    if (active !== undefined) {
      if (active.previewTimer !== undefined) window.clearTimeout(active.previewTimer);
      stopRecognition(active);
      stopStream(active.stream);
    }
  }, []);

  const runLocalPreview = useCallback(() => {
    const active = activeRef.current;
    if (active === undefined || active.recognition !== undefined || active.previewing || active.chunks.length === 0) {
      return;
    }
    active.previewTimer = undefined;
    active.previewing = true;
    void transcribe(recordedBlob(active))
      .then((text) => {
        if (activeRef.current !== active || text === '') return;
        liveTextRef.current = text;
        callbacksRef.current.onPreview(text);
      })
      .catch(() => {
        // Preview is best-effort; the final pass reports errors.
      })
      .finally(() => {
        active.previewing = false;
        if (activeRef.current === active) {
          active.previewTimer = window.setTimeout(runLocalPreview, PREVIEW_TRANSCRIBE_MS);
        }
      });
  }, []);

  const stop = useCallback(() => {
    const active = activeRef.current;
    if (active === undefined) return;
    if (timeoutRef.current !== undefined) {
      window.clearTimeout(timeoutRef.current);
      timeoutRef.current = undefined;
    }
    if (active.previewTimer !== undefined) {
      window.clearTimeout(active.previewTimer);
      active.previewTimer = undefined;
    }
    stopRecognition(active);
    setState('transcribing');
    if (active.recorder.state !== 'inactive') active.recorder.stop();

    void active.stopped
      .then(async (blob) => {
        clearActive();
        if (blob.size === 0) throw new Error('No microphone audio was recorded.');
        const text = (await transcribe(blob)) || liveTextRef.current;
        if (text !== '') callbacksRef.current.onFinal(text);
        else callbacksRef.current.onCancel();
        setError(undefined);
      })
      .catch((cause) => {
        const liveText = liveTextRef.current;
        if (liveText !== '') {
          callbacksRef.current.onFinal(liveText);
          setError(undefined);
        } else {
          callbacksRef.current.onCancel();
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      })
      .finally(() => {
        liveTextRef.current = '';
        setState('idle');
      });
  }, [clearActive]);

  const start = useCallback(() => {
    if (!supported) {
      setError('Voice input is not available in this webview.');
      return;
    }
    setError(undefined);
    setState('starting');
    void (async () => {
      const ensured = await ensureVoiceServer();
      if (!ensured.ok) throw new Error(ensured.error.message);

      let stream: MediaStream;
      try {
        stream = await navigator.mediaDevices.getUserMedia({ audio: micConstraint() });
      } catch {
        openMicPrivacySettings();
        throw new Error('Microphone access was denied.');
      }

      const chunks: Blob[] = [];
      const preferredMimeType = supportedMimeType();
      let recorder: MediaRecorder;
      try {
        recorder = new MediaRecorder(
          stream,
          preferredMimeType === undefined ? undefined : { mimeType: preferredMimeType },
        );
      } catch (cause) {
        stopStream(stream);
        throw cause;
      }
      const mimeType = preferredMimeType ?? (recorder.mimeType || 'audio/webm');
      const Recognition = speechRecognitionCtor();
      const recognition = Recognition === undefined ? undefined : new Recognition();
      if (recognition !== undefined) {
        recognition.continuous = true;
        recognition.interimResults = true;
        recognition.lang = 'en-US';
        recognition.onresult = (event) => {
          let text = '';
          for (let i = 0; i < event.results.length; i++) text += event.results[i]?.[0]?.transcript ?? '';
          text = text.trim();
          liveTextRef.current = text;
          callbacksRef.current.onPreview(text);
        };
        recognition.onerror = (event) => {
          if (event.error !== 'no-speech') {
            console.debug(`[chat-mic] live speech recognition skipped: ${event.error ?? event.message ?? 'unknown'}`);
          }
        };
        recognition.onend = () => {
          const active = activeRef.current;
          if (active?.recognition !== recognition || active.recorder.state !== 'recording') return;
          try {
            recognition.start();
          } catch {
            active.recognition = undefined;
            if (active.previewTimer === undefined) {
              active.previewTimer = window.setTimeout(runLocalPreview, PREVIEW_TRANSCRIBE_MS);
            }
          }
        };
      }
      const stopped = new Promise<Blob>((resolve, reject) => {
        recorder.ondataavailable = (event) => {
          if (event.data.size === 0) return;
          chunks.push(event.data);
          const active = activeRef.current;
          if (
            active !== undefined &&
            active.recognition === undefined &&
            active.previewTimer === undefined &&
            !active.previewing
          ) {
            active.previewTimer = window.setTimeout(runLocalPreview, PREVIEW_TRANSCRIBE_MS);
          }
        };
        recorder.onerror = () => reject(new Error('Microphone recording failed.'));
        recorder.onstop = () => resolve(new Blob(chunks, { type: mimeType }));
      });

      const active: ActiveRecording = { recorder, stream, chunks, mimeType, stopped, recognition, previewing: false };
      activeRef.current = active;
      liveTextRef.current = '';
      recorder.start(1_000);
      callbacksRef.current.onStart();
      if (recognition !== undefined) {
        try {
          recognition.start();
        } catch {
          active.recognition = undefined;
          active.previewTimer = window.setTimeout(runLocalPreview, PREVIEW_TRANSCRIBE_MS);
        }
      } else {
        active.previewTimer = window.setTimeout(runLocalPreview, PREVIEW_TRANSCRIBE_MS);
      }
      timeoutRef.current = window.setTimeout(stop, MAX_RECORDING_MS);
      setState('recording');
    })().catch((cause) => {
      clearActive();
      callbacksRef.current.onCancel();
      setError(cause instanceof Error ? cause.message : String(cause));
      setState('idle');
    });
  }, [clearActive, runLocalPreview, stop, supported]);

  useEffect(() => {
    return () => {
      const active = activeRef.current;
      if (active?.recorder.state === 'recording') active.recorder.stop();
      clearActive();
    };
  }, [clearActive]);

  return {
    state,
    error,
    supported,
    toggle: state === 'recording' ? stop : start,
    clearError: () => setError(undefined),
  };
}
