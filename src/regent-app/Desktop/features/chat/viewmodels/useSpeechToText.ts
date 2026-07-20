'use client';
// Composer push-to-talk: record via MediaRecorder, live-preview via browser
// SpeechRecognition (with a local-transcribe fallback loop), final transcript
// from the local voice server on stop. Plumbing lives in data/speechCapture.ts
// and data/wavEncode.ts.
import { useCallback, useEffect, useRef, useState } from 'react';
import { micConstraint } from '@/shared/infrastructure/mic';
import { openMicPrivacySettings } from '@/shared/infrastructure/opener';
import { ensureVoiceServer } from '@/shared/infrastructure/voice/ensure';
import { audioContextCtor } from '@/features/chat/data/wavEncode';
import {
  type ActiveRecording,
  MAX_RECORDING_MS,
  PREVIEW_TRANSCRIBE_MS,
  createLiveRecognition,
  recordedBlob,
  stopRecognition,
  stopStream,
  supportedMimeType,
  transcribe,
} from '@/features/chat/data/speechCapture';

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

export function useSpeechToText(callbacks: SpeechToTextCallbacks): SpeechToTextState {
  const [state, setState] = useState<SpeechState>('idle');
  const [error, setError] = useState<string>();
  const activeRef = useRef<ActiveRecording | undefined>(undefined);
  const timeoutRef = useRef<number | undefined>(undefined);
  const callbacksRef = useRef(callbacks);
  const liveTextRef = useRef('');
  // Environment-dependent, so it must NOT differ between the static prerender
  // and the first client render (hydration): start false, resolve in an effect.
  const [supported, setSupported] = useState(false);
  useEffect(() => {
    setSupported(
      navigator.mediaDevices !== undefined &&
        typeof MediaRecorder !== 'undefined' &&
        audioContextCtor() !== undefined,
    );
  }, []);

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
        // The server was warmed in the background at record start; make sure it
        // actually came up before transcribing (falls back to live text below).
        const ensured = await active.serverReady;
        if (!ensured.ok) throw new Error(ensured.error.message);
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
      // Acquire the mic and START RECORDING before touching the voice server.
      // Awaiting ensureVoiceServer() here — a health check plus a possible
      // cold-start spin-up — delayed live capture by hundreds of ms to seconds
      // after the tap, so the first word was spoken into dead air and clipped.
      // The server is only needed to TRANSCRIBE (on stop), so warm it in the
      // background and await it there instead.
      let stream: MediaStream;
      try {
        stream = await navigator.mediaDevices.getUserMedia({ audio: micConstraint() });
      } catch {
        openMicPrivacySettings();
        throw new Error('Microphone access was denied.');
      }
      const serverReady = ensureVoiceServer();

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
      const recognition = createLiveRecognition(
        () => activeRef.current,
        (text) => {
          liveTextRef.current = text;
          callbacksRef.current.onPreview(text);
        },
        (a) => {
          a.previewTimer = window.setTimeout(runLocalPreview, PREVIEW_TRANSCRIBE_MS);
        },
      );
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

      const active: ActiveRecording = { recorder, stream, chunks, mimeType, stopped, serverReady, recognition, previewing: false };
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
