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
//
// Split across: butlerSinks.ts / butlerPhase.ts (the CallSinks the loop
// drives) and data/butlerSetup.ts + data/captureWatchdog.ts (plumbing).
import { useCallback, useEffect, useRef, useState } from 'react';
import { ensureVoiceServer } from '@/shared/infrastructure/voice/ensure';
import { openMicPrivacySettings } from '@/shared/infrastructure/opener';
import { t } from '@/shared/i18n/t';
import { type ButlerState, initialButlerState } from '@/features/butler/domain/phase';
import { nextPresentation } from '@/features/butler/domain/presentation';
import { type VisualReadyGate } from '@/features/butler/domain/visualReady';
import { startCallLoop, type CallLoopController } from '@/features/butler/data/callLoop';
import { beginCallSession } from '@/features/butler/data/speechClient';
import { startCameraFrames } from '@/features/butler/data/cameraFrames';
import { openButlerStream, startWarmPoll } from '@/features/butler/data/butlerSetup';
import { startCaptureWatchdog } from '@/features/butler/data/captureWatchdog';
import { createButlerSinks } from '@/features/butler/viewmodels/butlerSinks';

export interface ButlerCall {
  readonly state: ButlerState;
  readonly analyserRef: React.RefObject<AnalyserNode | null>;
  /** Dismiss whatever backdrop holds the stage (globe or diagram) → voice. */
  readonly dismissStage: () => void;
  /** Called by the diagram after its first visible frame. */
  readonly markDiagramReady: () => void;
  readonly micMuted: boolean;
  readonly toggleMic: () => void;
  readonly submitText: (text: string) => void;
}

export function useButlerCall(): ButlerCall {
  const [state, setState] = useState<ButlerState>(initialButlerState);
  const [micMuted, setMicMuted] = useState(false);
  const [captureGeneration, setCaptureGeneration] = useState(0);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const loopRef = useRef<CallLoopController | null>(null);
  const audioTrackRef = useRef<MediaStreamTrack | null>(null);
  const micMutedRef = useRef(false);
  // Capture watchdog rebuilds stay inside the same Butler conversation; a
  // fresh hook instance (modal reopened) gets a fresh persisted session.
  const sessionStartRef = useRef<Promise<boolean> | null>(null);
  // The RAW reply (```present block intact) — turn-end parses the spec from it.
  const fullReplyRef = useRef('');
  // Latest transcript + prior phase, read by the async place resolver (which
  // runs after the sync archive and can't reach the reducer's `s`).
  const heardRef = useRef('');
  const prevPhaseRef = useRef('connecting');
  // Once a turn's diagram spec has been raised (mid-stream), don't re-raise it.
  const specShownRef = useRef(false);
  // The first audio chunk waits here for visual-first explanation turns. The
  // gate is released by the diagram's visible frame, or by the first reply
  // proving there is no leading diagram.
  const visualGateRef = useRef<VisualReadyGate | null>(null);
  const visualDecisionRef = useRef(false);
  const visualExpectedRef = useRef(false);

  const markDiagramReady = useCallback(() => {
    visualGateRef.current?.release();
    visualGateRef.current = null;
  }, []);

  useEffect(() => {
    let cancelled = false;
    const isCancelled = () => cancelled;
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
      // Server boot and the OS media prompt are independent. Running them in
      // parallel removes their additive startup delay, especially on first use.
      const [ensured, media] = await Promise.all([
        ensureVoiceServer().then(async (result) => {
          if (!result.ok) return { result, sessionStarted: false };
          // React StrictMode mounts the effect twice in development. Cache the
          // in-flight handshake (not only its eventual boolean) so those two
          // effects cannot create two persisted sessions for one opening.
          sessionStartRef.current ??= beginCallSession();
          const sessionStarted = await sessionStartRef.current;
          if (!sessionStarted) sessionStartRef.current = null; // retry after a capture rebuild
          return { result, sessionStarted };
        }),
        openButlerStream().then(
          (stream) => ({ ok: true as const, stream }),
          (error: unknown) => ({ ok: false as const, error }),
        ),
      ]);
      if (cancelled) {
        if (media.ok) for (const track of media.stream.getTracks()) track.stop();
        return;
      }
      if (!ensured.result.ok) {
        if (media.ok) for (const track of media.stream.getTracks()) track.stop();
        const message = ensured.result.error.message;
        setState((s) => ({ ...s, error: message }));
        return;
      }
      if (!media.ok) {
        setState((s) => ({ ...s, error: t().butler.micDenied }));
        openMicPrivacySettings();
        return;
      }
      if (!ensured.sessionStarted) {
        setState((s) => ({ ...s, error: t().butler.sessionFailed }));
      }
      cleanups.push(startWarmPoll(isCancelled, setState));
      // Pin the saved input device and keep camera optional (openButlerStream
      // already fell back to mic-only if camera permission was unavailable).
      const stream = media.stream;
      cleanups.push(() => {
        for (const track of stream.getTracks()) track.stop();
      });
      audioTrackRef.current = stream.getAudioTracks()[0] ?? null;
      if (audioTrackRef.current) audioTrackRef.current.enabled = !micMutedRef.current;
      // Feed the agent's camera_capture tool while the call runs (no-op
      // when the camera fallback stripped the video track).
      cleanups.push(startCameraFrames(stream));

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
      // Reply audio renders through this gain so a suspected barge can DUCK
      // the voice (keep it playing, quietly) while the server verifies the
      // interruption — a noise verdict un-ducks and nothing is lost.
      const playGain = playCtx.createGain();
      playGain.connect(playCtx.destination);

      const sinks = createButlerSinks({
        isCancelled,
        setState,
        markDiagramReady,
        analyser,
        playAnalyser,
        analyserRef,
        heardRef,
        fullReplyRef,
        prevPhaseRef,
        specShownRef,
        visualGateRef,
        visualDecisionRef,
        visualExpectedRef,
      });
      const loop = startCallLoop(
        ctx,
        source,
        analyser,
        {
          ctx: playCtx,
          node: playAnalyser,
          out: playGain,
          duck: (on) => {
            playGain.gain.value = on ? 0.15 : 1;
          },
        },
        sinks,
      );
      loopRef.current = loop;
      loop.setMuted(micMutedRef.current);
      cleanups.push(
        startCaptureWatchdog({
          ctx,
          isCancelled,
          lastAudioFrameAt: loop.lastAudioFrameAt,
          audioTrack: () => audioTrackRef.current,
          requestRestart: () => setCaptureGeneration((generation) => generation + 1),
        }),
      );
      cleanups.push(() => {
        loop.dispose();
        source.disconnect();
        if (loopRef.current === loop) loopRef.current = null;
        audioTrackRef.current = null;
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
      markDiagramReady();
      for (const dispose of cleanups.reverse()) dispose();
      analyserRef.current = null;
    };
  }, [markDiagramReady, captureGeneration]);

  const dismissStage = useCallback(() => {
    markDiagramReady();
    setState((s) => ({ ...s, presentation: nextPresentation(s.presentation, { type: 'voice' }) }));
  }, [markDiagramReady]);

  const toggleMic = useCallback(() => {
    const next = !micMutedRef.current;
    micMutedRef.current = next;
    if (audioTrackRef.current) audioTrackRef.current.enabled = !next;
    loopRef.current?.setMuted(next);
    setMicMuted(next);
  }, []);

  const submitText = useCallback((text: string) => loopRef.current?.submitText(text), []);

  return {
    state,
    analyserRef,
    dismissStage,
    markDiagramReady,
    micMuted,
    toggleMic,
    submitText,
  };
}
