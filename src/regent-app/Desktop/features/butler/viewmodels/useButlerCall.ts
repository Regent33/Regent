'use client';
// Butler call lifecycle: ensure the voice server, open the mic (echo
// cancellation ON — barge-in depends on it), wire the VAD loop, and expose
// phase/captions plus the analyser the particle core visualizes. Everything
// tears down on unmount; a cancelled async step never touches state.
import { useEffect, useRef, useState } from 'react';
import { ensureVoiceServer } from '@/shared/infrastructure/voice/ensure';
import { t } from '@/shared/i18n/t';
import { type ButlerState, initialButlerState } from '@/features/butler/domain/phase';
import { startCallLoop } from '@/features/butler/data/callLoop';

export interface ButlerCall {
  readonly state: ButlerState;
  readonly analyserRef: React.RefObject<AnalyserNode | null>;
}

export function useButlerCall(): ButlerCall {
  const [state, setState] = useState<ButlerState>(initialButlerState);
  const analyserRef = useRef<AnalyserNode | null>(null);

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;

    void (async () => {
      const ensured = await ensureVoiceServer();
      if (cancelled) return;
      if (!ensured.ok) {
        setState((s) => ({ ...s, error: ensured.error.message }));
        return;
      }
      let stream: MediaStream;
      try {
        stream = await navigator.mediaDevices.getUserMedia({
          audio: { echoCancellation: true, noiseSuppression: true },
        });
      } catch {
        if (!cancelled) setState((s) => ({ ...s, error: t().butler.micDenied }));
        return;
      }
      if (cancelled) {
        for (const track of stream.getTracks()) track.stop();
        return;
      }
      const ctx = new AudioContext();
      const source = ctx.createMediaStreamSource(stream);
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      source.connect(analyser);
      analyserRef.current = analyser;

      const proc = startCallLoop(ctx, source, analyser, {
        setPhase: (phase) => {
          if (!cancelled) setState((s) => ({ ...s, phase }));
        },
        setHeard: (heard) => {
          if (!cancelled) setState((s) => ({ ...s, heard }));
        },
        setReply: (reply) => {
          if (!cancelled) setState((s) => ({ ...s, reply }));
        },
        setError: (error) => {
          if (!cancelled) setState((s) => ({ ...s, error }));
        },
      });
      setState((s) => ({ ...s, phase: 'listening' }));

      cleanup = () => {
        proc.disconnect();
        source.disconnect();
        void ctx.close();
        for (const track of stream.getTracks()) track.stop();
      };
    })();

    return () => {
      cancelled = true;
      cleanup?.();
      analyserRef.current = null;
    };
  }, []);

  return { state, analyserRef };
}
