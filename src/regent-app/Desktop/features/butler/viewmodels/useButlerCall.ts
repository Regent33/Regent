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
import { useCallback, useEffect, useRef, useState } from 'react';
import { ensureVoiceServer } from '@/shared/infrastructure/voice/ensure';
import { openMicPrivacySettings } from '@/shared/infrastructure/opener';
import { micConstraint } from '@/shared/infrastructure/mic';
import { t } from '@/shared/i18n/t';
import { type ButlerState, initialButlerState } from '@/features/butler/domain/phase';
import { startCallLoop } from '@/features/butler/data/callLoop';
import { placeIntent } from '@/features/butler/data/geocode';
import { extractLinks } from '@/features/butler/data/links';

export interface ButlerCall {
  readonly state: ButlerState;
  readonly analyserRef: React.RefObject<AnalyserNode | null>;
  readonly dismissMap: () => void;
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
    // Regent's voice renders through its OWN capture-free context: rendering
    // through the mic context (a communications session — echo-cancelled
    // capture) put TTS and, via driver voice-call DSP, other apps' audio onto
    // the phone-call path: muffled voice, ducked music (see callLoop).
    const playCtx = new AudioContext();
    cleanups.push(() => void playCtx.close());

    const unstick = () => {
      void playCtx.resume();
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
        // Pin the user's chosen input device (Voice settings) when set.
        stream = await navigator.mediaDevices.getUserMedia({ audio: micConstraint() });
      } catch {
        if (!cancelled) {
          setState((s) => ({ ...s, error: t().butler.micDenied }));
          // A blocked mic can't re-summon the OS popup — jump the user to
          // the exact Windows privacy page instead of describing the path.
          openMicPrivacySettings();
        }
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
      if (playCtx.state === 'suspended') await playCtx.resume().catch(() => undefined);
      // Definitive liveness check, OUTSIDE the audio callback (a dead graph
      // never fires the in-loop watchdogs): if the clock hasn't advanced in
      // 4s, the engine is not running — say so.
      setTimeout(() => {
        if (!cancelled && ctx.currentTime === 0) {
          setState((s) => ({ ...s, error: t().butler.audioStuck }));
        }
      }, 4000);
      const source = ctx.createMediaStreamSource(stream);
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 256;
      source.connect(analyser);
      analyserRef.current = analyser;
      // Playback lives in playCtx, so it needs its own analyser; the ref
      // swaps between the two per phase (VoiceDots reads it every frame), so
      // the dots follow your voice while listening and Regent's while speaking.
      const playAnalyser = playCtx.createAnalyser();
      playAnalyser.fftSize = 256;

      const proc = startCallLoop(ctx, source, analyser, { ctx: playCtx, node: playAnalyser }, {
        setPhase: (phase) => {
          if (cancelled) return;
          analyserRef.current = phase === 'speaking' ? playAnalyser : analyser;
          setState((s) => {
            // Turn finished (busy → listening): archive the exchange into the
            // caption log for the Conversation window.
            if (phase === 'listening' && s.phase !== 'listening' && s.reply !== '') {
              // Turn done: archive the exchange and surface any links Regent
              // mentioned as result cards (only replace when there are new ones).
              const found = extractLinks(s.reply);
              return {
                ...s,
                phase,
                reply: '',
                log: [...s.log, { who: 'regent', text: s.reply }],
                links: found.length > 0 ? found : s.links,
              };
            }
            return { ...s, phase };
          });
        },
        setHeard: (heard) => {
          if (cancelled) return;
          setState((s) => ({
            ...s,
            heard,
            log: [...s.log, { who: 'you', text: heard }],
            // A map-shaped ask brings the backdrop up (and re-flies on a new
            // place); anything else leaves the current map alone.
            mapQuery: placeIntent(heard) ?? s.mapQuery,
          }));
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

  const dismissMap = useCallback(() => setState((s) => ({ ...s, mapQuery: null })), []);

  return { state, analyserRef, dismissMap };
}
