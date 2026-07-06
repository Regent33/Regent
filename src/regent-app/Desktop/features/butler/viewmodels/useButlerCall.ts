'use client';
// Butler call lifecycle: ensure the voice server, open the mic (echo
// cancellation ON — barge-in depends on it), wire the VAD loop, and expose
// phase/captions plus the analyser the voice mark visualizes.
//
// The AudioContext is created SYNCHRONOUSLY at mount — inside the opening
// click's transient-activation window — because WebView2 hands back a
// permanently-suspended context when creation happens after long awaits
// (server probe + mic prompt), and a suspended graph = dead VAD + frozen
// visualizer with no error anywhere ("stuck on Listening"). If it still
// reports suspended after setup, that state is SHOWN and any click/key
// resumes it.
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
    const cleanups: Array<() => void> = [];

    // Synchronous — see module comment. Everything async comes after.
    const ctx = new AudioContext();
    cleanups.push(() => void ctx.close());

    const unstick = () => {
      void ctx.resume().then(() => {
        if (!cancelled && ctx.state === 'running') {
          setState((s) => (s.error === t().butler.audioStuck ? { ...s, error: null } : s));
        }
      });
    };
    window.addEventListener('pointerdown', unstick);
    window.addEventListener('keydown', unstick);
    cleanups.push(() => {
      window.removeEventListener('pointerdown', unstick);
      window.removeEventListener('keydown', unstick);
    });

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
      cleanups.push(() => {
        for (const track of stream.getTracks()) track.stop();
      });

      if (ctx.state === 'suspended') await ctx.resume().catch(() => undefined);
      const source = ctx.createMediaStreamSource(stream);
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      source.connect(analyser);
      analyserRef.current = analyser;

      const proc = startCallLoop(ctx, source, analyser, {
        setPhase: (phase) => {
          if (cancelled) return;
          setState((s) => {
            // Turn finished (busy → listening): archive the exchange into the
            // caption log for the Conversation window.
            if (phase === 'listening' && s.phase !== 'listening' && s.reply !== '') {
              return { ...s, phase, reply: '', log: [...s.log, { who: 'regent', text: s.reply }] };
            }
            return { ...s, phase };
          });
        },
        setHeard: (heard) => {
          if (cancelled) return;
          setState((s) => ({ ...s, heard, log: [...s.log, { who: 'you', text: heard }] }));
        },
        setReply: (reply) => {
          if (!cancelled) setState((s) => ({ ...s, reply }));
        },
        setError: (error) => {
          if (!cancelled) setState((s) => ({ ...s, error }));
        },
      });
      cleanups.push(() => {
        proc.disconnect();
        source.disconnect();
      });
      setState((s) => ({
        ...s,
        phase: 'listening',
        // Still suspended after the resume attempts → say so instead of
        // sitting silent; the pointer/key listener above clears it.
        error: ctx.state === 'running' ? s.error : t().butler.audioStuck,
      }));
    })();

    return () => {
      cancelled = true;
      for (const dispose of cleanups.reverse()) dispose();
      analyserRef.current = null;
    };
  }, []);

  return { state, analyserRef };
}
