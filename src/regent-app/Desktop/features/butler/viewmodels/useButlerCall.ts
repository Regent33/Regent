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
import { sendQuestionAnswer } from '@/features/butler/data/callAnswer';
import type { QuestionnaireAnswer } from '@/shared/kernel/questionnaire';

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
  /** Answer the open question card. Tapping is a convenience — the caller can
   *  always just say it, which the mic path carries as the next turn. */
  readonly answerQuestion: (answer: QuestionnaireAnswer) => void;
}

export function useButlerCall(options: { onExit?: () => void } = {}): ButlerCall {
  const [state, setState] = useState<ButlerState>(initialButlerState);
  // Held in a ref, never in the effect's deps: a caller passing an inline
  // arrow would otherwise change identity every render and tear down and
  // rebuild the entire audio graph (mic, contexts, VAD loop) each time.
  const onExitRef = useRef(options.onExit);
  onExitRef.current = options.onExit;
  const [micMuted, setMicMuted] = useState(false);
  const [captureGeneration, setCaptureGeneration] = useState(0);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const loopRef = useRef<CallLoopController | null>(null);
  const audioTrackRef = useRef<MediaStreamTrack | null>(null);
  const micMutedRef = useRef(false);
  // Capture watchdog rebuilds stay inside the same Butler conversation; a
  // fresh hook instance (modal reopened) gets a fresh persisted session.
  const sessionStartRef = useRef<ReturnType<typeof beginCallSession> | null>(null);
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
  // Mirrors "the map holds the stage" for the sinks, which run outside React
  // and need it synchronously. While it is up, a follow-up can name a new
  // place without repeating an explicit cue ("what about Tokyo").
  const mapOpenRef = useRef(false);
  mapOpenRef.current = state.presentation.kind === 'map';

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
          if (!result.ok) return { result, session: undefined };
          // React StrictMode mounts the effect twice in development. Cache the
          // in-flight handshake so those effects cannot create two sessions.
          sessionStartRef.current ??= beginCallSession();
          const session = await sessionStartRef.current;
          if (!session.ok) sessionStartRef.current = null;
          return { result, session };
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
      if (ensured.session === undefined || !ensured.session.ok) {
        for (const track of media.stream.getTracks()) track.stop();
        const message =
          ensured.session?.error.kind === 'voice-session-timeout'
            ? t().butler.sessionTimeout
            : ensured.session?.error.message || t().butler.sessionFailed;
        setState((s) => ({ ...s, error: message }));
        return;
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
      // The analyser taps the gain's OUTPUT, not the raw source. It feeds the
      // echo estimator, whose whole model is "mic echo ÷ what the speaker is
      // emitting" — so it must see the ducked level, which is what actually
      // reaches the room. Tapped pre-gain (the source directly), ducking to
      // 15% made the mic drop while the reported render stayed full; that
      // ratio reads as "coupling collapsed", is learned at full rate, and on
      // un-duck the real echo towers over the now-tiny prediction — a barge
      // that re-fires and poisons the estimate further every cycle.
      playGain.connect(playAnalyser);
      // The echo estimator gets its OWN tap, and a much longer one. It divides
      // a mic frame by the playback level over the same instant — but a mic
      // frame is 2048 samples (~43ms) and this analyser was 256 (~5.3ms), so
      // it was dividing 43ms of measured echo by a 5.3ms SNAPSHOT of the
      // render. Regent's own speech is full of stop closures and word gaps
      // tens of ms long; whenever that snapshot landed in one it read ~0 while
      // the mic window still carried the echo of the syllable just before it.
      // The prediction collapsed, the residual spiked over the barge gate, and
      // the correlation veto — which returns 0 when the render window looks
      // silent — was disabled at the same instant, so both the gate and its
      // backstop failed together. Simulated over ~17s of speech: 6 false
      // barges at 5.3ms against 1 at 43ms. Each one ducks the reply to 4%,
      // which is what "Regent goes muted mid-sentence" actually was.
      // Separate from the dots' analyser on purpose: a level meter wants a
      // short window, this wants the mic's.
      const echoAnalyser = playCtx.createAnalyser();
      echoAnalyser.fftSize = 2048;
      playGain.connect(echoAnalyser);

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
        mapOpenRef,
        onExit: () => onExitRef.current?.(),
      });
      const loop = startCallLoop(
        ctx,
        source,
        analyser,
        {
          ctx: playCtx,
          node: echoAnalyser,
          out: playGain,
          // A barge confirms in ~300ms, but it cannot CUT the reply until the
          // server's ASR has judged it — ~690ms of endpoint silence plus
          // 0.4–2.6s of whisper. Ducking to a level still clearly audible
          // meant Regent talked over the caller for that whole window: "barge
          // in works but it has delay". Dropping to effectively silent makes
          // it FEEL like he stopped the moment they spoke, while verification
          // continues underneath — a noise verdict simply ramps him back.
          // (Cost: a false positive loses ~1s of the explanation to inaudible
          // playback instead of merely quiet. False positives are now rare —
          // residual echo veto, caller-level gate, and the self-echo check.)
          // Ramped, not stepped, so neither edge clicks.
          duck: (on) => {
            const at = playCtx.currentTime;
            playGain.gain.cancelScheduledValues(at);
            playGain.gain.setValueAtTime(playGain.gain.value, at);
            playGain.gain.linearRampToValueAtTime(on ? 0.04 : 1, at + 0.06);
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

  // Tapping an option answers the paused turn. The card comes down either way:
  // a `resolved: false` means the question already timed out or a second tap
  // raced the first, and leaving it up would offer taps that do nothing.
  const answerQuestion = useCallback((answer: QuestionnaireAnswer) => {
    setState((s) => ({ ...s, question: null }));
    void sendQuestionAnswer(answer);
  }, []);

  return {
    state,
    analyserRef,
    dismissStage,
    markDiagramReady,
    micMuted,
    toggleMic,
    submitText,
    answerQuestion,
  };
}
