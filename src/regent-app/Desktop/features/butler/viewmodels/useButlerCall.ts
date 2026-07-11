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
import { nextPresentation } from '@/features/butler/domain/presentation';
import { splitLinks } from '@/features/butler/domain/content';
import { startCallLoop } from '@/features/butler/data/callLoop';
import { hasPlaceCandidate, resolvePlaces } from '@/features/butler/data/geocode';
import { extractLinks } from '@/features/butler/data/links';
import { extractPresentSpec, stripPresentTail } from '@/features/butler/data/presentSpec';

export interface ButlerCall {
  readonly state: ButlerState;
  readonly analyserRef: React.RefObject<AnalyserNode | null>;
  /** Dismiss whatever backdrop holds the stage (globe or diagram) → voice. */
  readonly dismissStage: () => void;
}

export function useButlerCall(): ButlerCall {
  const [state, setState] = useState<ButlerState>(initialButlerState);
  const analyserRef = useRef<AnalyserNode | null>(null);
  // The RAW reply (```present block intact) — turn-end parses the spec from it.
  const fullReplyRef = useRef('');
  // Latest transcript + prior phase, read by the async place resolver (which
  // runs after the sync archive and can't reach the reducer's `s`).
  const heardRef = useRef('');
  const prevPhaseRef = useRef('connecting');
  // Once a turn's diagram spec has been raised (mid-stream), don't re-raise it.
  const specShownRef = useRef(false);

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
          const wasListening = prevPhaseRef.current === 'listening';
          prevPhaseRef.current = phase;
          // Turn finished (busy → listening): archive the exchange and route the
          // stage. Parse (and remove) any ```present diagram spec from the RAW
          // reply first; everything downstream works on the cleaned prose.
          if (phase === 'listening' && !wasListening && fullReplyRef.current !== '') {
            const { spec, text } = extractPresentSpec(fullReplyRef.current);
            const found = extractLinks(text);
            const { promoted, plain } = splitLinks(found);
            const heard = heardRef.current;
            // Might any of this turn name a place? (cheap sync check) — if so we
            // hold the current stage and let the async geocoder decide, rather
            // than flip to voice and flicker.
            const maybePlace = !spec && (hasPlaceCandidate(heard) || hasPlaceCandidate(text));
            setState((s) => {
              // Precedence: diagram spec → diagram; else promoted content →
              // windows; else (no place candidate and nothing else) a bare turn
              // yields the stage back to voice; else hold for the async lookup.
              const presentation = spec
                ? nextPresentation(s.presentation, { type: 'diagram', spec })
                : promoted.length > 0
                  ? nextPresentation(s.presentation, { type: 'content' })
                  : !maybePlace && found.length === 0 && s.presentation.kind !== 'voice'
                    ? nextPresentation(s.presentation, { type: 'voice' })
                    : s.presentation;
              return {
                ...s,
                phase,
                reply: '',
                log: [...s.log, { who: 'regent', text }],
                links: plain.length > 0 ? plain : s.links,
                content: promoted.length > 0 ? promoted : s.content,
                presentation,
              };
            });
            // Geocode-gate the whole turn: any candidate that resolves to a real
            // place raises the globe with those pins; none resolving leaves a
            // stale globe only if the turn truly moved on (no links).
            if (!spec) {
              void (async () => {
                const places = await resolvePlaces(`${heard}\n${text}`);
                if (cancelled) return;
                if (places.length > 0) {
                  setState((s) => ({ ...s, presentation: nextPresentation(s.presentation, { type: 'places', places }) }));
                } else if (found.length === 0) {
                  setState((s) =>
                    s.presentation.kind === 'map'
                      ? { ...s, presentation: nextPresentation(s.presentation, { type: 'voice' }) }
                      : s,
                  );
                }
              })();
            }
            return;
          }
          setState((s) => ({ ...s, phase }));
        },
        setHeard: (heard) => {
          if (cancelled) return;
          heardRef.current = heard;
          specShownRef.current = false; // new turn — allow the next spec to raise
          setState((s) => ({ ...s, heard, log: [...s.log, { who: 'you', text: heard }] }));
          // Raise the globe as you speak — but only once a candidate actually
          // geocodes, so "where's my file" never opens a map.
          void (async () => {
            const places = await resolvePlaces(heard);
            if (!cancelled && places.length > 0) {
              setState((s) => ({ ...s, presentation: nextPresentation(s.presentation, { type: 'places', places }) }));
            }
          })();
        },
        setReply: (reply) => {
          if (cancelled) return;
          fullReplyRef.current = reply;
          // Caption drops a partial/complete spec block (no JSON flash).
          setState((s) => ({ ...s, reply: stripPresentTail(reply) }));
          // Raise the diagram the instant its block finishes STREAMING — text
          // completes well before the TTS audio drains (which is when the turn
          // ends), so the diagram appears while Regent is still speaking rather
          // than after. Idempotent per turn via specShownRef.
          if (!specShownRef.current) {
            const { spec } = extractPresentSpec(reply);
            if (spec) {
              specShownRef.current = true;
              setState((s) => ({
                ...s,
                presentation: nextPresentation(s.presentation, { type: 'diagram', spec }),
              }));
            }
          }
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

  const dismissStage = useCallback(
    () => setState((s) => ({ ...s, presentation: nextPresentation(s.presentation, { type: 'voice' }) })),
    [],
  );

  return { state, analyserRef, dismissStage };
}
