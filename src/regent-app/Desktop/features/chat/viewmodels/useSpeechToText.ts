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

interface ActiveRecording {
  readonly recorder: MediaRecorder;
  readonly stream: MediaStream;
  readonly stopped: Promise<Blob>;
}

const MAX_RECORDING_MS = 60_000;
const TRANSCRIBE_TIMEOUT_MS = 120_000;
const TARGET_SAMPLE_RATE = 16_000;
const MIME_TYPES = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg;codecs=opus', 'audio/mp4'];

function supportedMimeType(): string | undefined {
  return MIME_TYPES.find((type) => MediaRecorder.isTypeSupported(type));
}

function audioContextCtor(): typeof AudioContext | undefined {
  return window.AudioContext ?? (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
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

export function useSpeechToText(onText: (text: string) => void): SpeechToTextState {
  const [state, setState] = useState<SpeechState>('idle');
  const [error, setError] = useState<string>();
  const activeRef = useRef<ActiveRecording | undefined>(undefined);
  const timeoutRef = useRef<number | undefined>(undefined);
  const onTextRef = useRef(onText);
  const supported =
    typeof navigator !== 'undefined' &&
    navigator.mediaDevices !== undefined &&
    typeof MediaRecorder !== 'undefined' &&
    audioContextCtor() !== undefined;

  useEffect(() => {
    onTextRef.current = onText;
  }, [onText]);

  const clearActive = useCallback(() => {
    if (timeoutRef.current !== undefined) {
      window.clearTimeout(timeoutRef.current);
      timeoutRef.current = undefined;
    }
    const active = activeRef.current;
    activeRef.current = undefined;
    if (active !== undefined) stopStream(active.stream);
  }, []);

  const stop = useCallback(() => {
    const active = activeRef.current;
    if (active === undefined) return;
    if (timeoutRef.current !== undefined) {
      window.clearTimeout(timeoutRef.current);
      timeoutRef.current = undefined;
    }
    setState('transcribing');
    if (active.recorder.state !== 'inactive') active.recorder.stop();

    void active.stopped
      .then(async (blob) => {
        clearActive();
        if (blob.size === 0) throw new Error('No microphone audio was recorded.');
        const text = await transcribe(blob);
        if (text !== '') onTextRef.current(text);
        setError(undefined);
      })
      .catch((cause) => {
        setError(cause instanceof Error ? cause.message : String(cause));
      })
      .finally(() => {
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
      const mimeType = supportedMimeType();
      let recorder: MediaRecorder;
      try {
        recorder = new MediaRecorder(stream, mimeType === undefined ? undefined : { mimeType });
      } catch (cause) {
        stopStream(stream);
        throw cause;
      }
      const stopped = new Promise<Blob>((resolve, reject) => {
        recorder.ondataavailable = (event) => {
          if (event.data.size > 0) chunks.push(event.data);
        };
        recorder.onerror = () => reject(new Error('Microphone recording failed.'));
        recorder.onstop = () => resolve(new Blob(chunks, { type: mimeType ?? 'audio/webm' }));
      });

      activeRef.current = { recorder, stream, stopped };
      recorder.start();
      timeoutRef.current = window.setTimeout(stop, MAX_RECORDING_MS);
      setState('recording');
    })().catch((cause) => {
      clearActive();
      setError(cause instanceof Error ? cause.message : String(cause));
      setState('idle');
    });
  }, [clearActive, stop, supported]);

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
