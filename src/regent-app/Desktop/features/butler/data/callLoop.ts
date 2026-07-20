// The turn-based local call — port of regent-web/hooks/localCall.ts: an
// energy-gated VAD loop that detects when the caller stops talking, POSTs the
// utterance to /call/turn, and plays the streamed reply (with barge-in).
// Tuning is local to the desktop capture path: short frames + onset confirmation
// reject noise without clipping the beginning of real speech.
// Split across: callTypes.ts (contracts), loopState.ts (shared mutable state),
// bargeIn.ts (busy branch), capture.ts (onset/endpoint), callTurn.ts (transport).
import { sustainGate, voiceGate } from '@/features/butler/domain/vad';
import { isSelfEcho } from '@/features/butler/domain/selfEcho';
import type { CallLoopController, CallSinks, PlaybackSink } from '@/features/butler/data/callTypes';
import { handleBusyFrame } from '@/features/butler/data/bargeIn';
import { handleCaptureFrame, handleIdleFrame } from '@/features/butler/data/capture';
import { runTextTurn, runTurn } from '@/features/butler/data/callTurn';
import {
  PROCESSOR_SAMPLES,
  createLoopState,
  frameRms,
  recalibrateRoom,
  resetCapture,
  resetInterrupt,
} from '@/features/butler/data/loopState';

export type { CallLoopController, CallSinks, PlaybackSink } from '@/features/butler/data/callTypes';

/**
 * Energy-gated turn loop. Returns the processor node so the caller can
 * disconnect it on cleanup.
 */
export function startCallLoop(
  ctx: AudioContext,
  source: MediaStreamAudioSourceNode,
  node: AnalyserNode,
  playback: PlaybackSink,
  sinks: CallSinks,
): CallLoopController {
  // ponytail: ScriptProcessorNode is deprecated but works everywhere and needs
  // no separate worklet file; swap for an AudioWorklet if it ever drops.
  const proc = ctx.createScriptProcessor(PROCESSOR_SAMPLES, 1, 1);
  source.connect(proc);
  proc.connect(ctx.destination); // keeps the node pulling; it outputs silence

  const s = createLoopState(playback.node.fftSize);
  let abort: AbortController | null = null;
  let verifyAbort: AbortController | null = null;
  // What Regent is saying right now. The server's ASR judges a suspected
  // interruption without it — so it confidently transcribes his own voice
  // returning through the speakers. Held here, the client can tell the two
  // apart (see isSelfEcho); cleared when a new turn begins.
  let spokenReply = '';
  const trackedSinks: CallSinks = {
    ...sinks,
    setReply: (reply) => {
      spokenReply = reply;
      sinks.setReply(reply);
    },
  };
  console.debug(`[butler] VAD loop started (ctx.state=${ctx.state})`);

  const stopTurn = () => {
    abort?.abort();
    abort = null;
    if (s.playing.src) {
      try {
        s.playing.src.stop(); // fires onended → resolves the awaiting playPcm
      } catch {
        // already stopped
      }
      s.playing.src = null;
    }
  };

  // Abandon a pending barge verification (mute, typed turn, dispose): the
  // suspect audio is discarded and the playing reply returns to full volume.
  const cancelVerify = () => {
    verifyAbort?.abort();
    verifyAbort = null;
    s.verify = null;
    playback.duck(false);
  };

  // A suspected interruption reached its endpoint: POST it. The server gates
  // it BEFORE the agent is touched (VAD → ASR → only real speech reaches the
  // deacon), so a noise verdict leaves the in-flight reply fully intact.
  const verifyTurn = (frames: Float32Array[]) => {
    const vAbort = new AbortController();
    verifyAbort = vAbort;
    let promoted = false;
    const promote = () => {
      if (promoted || vAbort.signal.aborted) return;
      promoted = true;
      stopTurn(); // the interruption is REAL — now the old reply dies
      playback.duck(false);
      s.turnGen += 1;
      abort = vAbort;
      verifyAbort = null;
      s.busy = true;
      s.busyFrames = 0;
      s.replyAudible = false;
      // Re-issue the phase this turn already announced. runTurn sets
      // 'thinking' before the server answers — i.e. before `heard`, so before
      // this promotion — and the guard below rightly swallowed it to protect
      // the then-live turn. Nothing re-sent it, so the UI kept displaying the
      // OLD turn's 'speaking' with no audio and the previous answer still on
      // screen: the reported "it seems like it's stuck". It is thinking, so
      // say so (this also runs the normal new-turn reset in makeSetPhase).
      sinks.setPhase('thinking');
    };
    // Until the server proves this was speech (a `heard` line), the verify
    // stream must not disturb the live turn: phases and errors are held back.
    // Audio can only follow `heard` (server emits it first), so by the time
    // anything plays, the old turn is already stopped.
    const vSinks = {
      ...trackedSinks,
      setHeard: (heard: string) => {
        // The server proved this was SPEECH; only the client can tell whose.
        // A verbatim run of Regent's own reply means the mic heard the
        // speakers — abandon the verification and let him finish talking.
        if (isSelfEcho(heard, spokenReply)) {
          console.debug(`[butler] self-echo rejected: ${heard.slice(0, 60)}`);
          vAbort.abort();
          playback.duck(false);
          return;
        }
        promote();
        sinks.setHeard(heard);
      },
      setPhase: (p: Parameters<CallSinks['setPhase']>[0]) => {
        if (promoted) sinks.setPhase(p);
      },
      setError: (e: string | null) => {
        if (promoted || e === null) sinks.setError(e);
      },
    };
    void runTurn(frames, ctx.sampleRate, playback, vSinks, vAbort.signal, s.playing, () => {
      if (abort === vAbort) s.busyFrames = 0;
    })
      .then(() => {
        // Noise verdict / silent failure: nothing was promoted, so the reply
        // kept playing — just restore its volume.
        if (!promoted) playback.duck(false);
      })
      .finally(() => {
        if (verifyAbort === vAbort) verifyAbort = null;
        if (promoted && abort === vAbort) s.busy = false;
      });
  };

  const busyDeps = { playback, sinks, stopTurn, verifyTurn };

  const beginVoiceTurn = (utterance: Float32Array[]) => {
    resetInterrupt(s);
    s.busy = true;
    s.busyFrames = 0;
    s.replyAudible = false;
    spokenReply = ''; // new turn — the previous reply is no longer being spoken
    abort = new AbortController();
    const myGen = ++s.turnGen;
    void runTurn(utterance, ctx.sampleRate, playback, trackedSinks, abort.signal, s.playing, () => {
      if (myGen === s.turnGen) s.busyFrames = 0; // streamed line → watchdog reset
    })
      .then((outcome) => {
        // Server said noise — KEEP the learned floor. Zeroing it here (the
        // old recalibrateRoom) dropped the gate to its hair-trigger minimum
        // right after the room just proved noisy, so the same background
        // re-triggered instantly (the noise → reject → noise loop). The
        // idle EMA still re-lowers the floor if the room actually quiets.
        if (myGen === s.turnGen && outcome === 'noise') resetCapture(s);
      })
      .finally(() => {
        if (myGen === s.turnGen) s.busy = false; // ignore a turn we barged over
      });
  };

  proc.onaudioprocess = (e) => {
    s.lastFrameAt = performance.now();
    const d = e.inputBuffer.getChannelData(0);
    const rms = frameRms(d);
    // Gates use room tone learned while idle. Never learn while `busy`:
    // residual TTS echo otherwise raises the floor throughout a long reply and
    // eventually puts barge-in above the caller's microphone level.
    const gate = voiceGate(s.noiseFloor);
    const sustain = sustainGate(s.noiseFloor);

    s.dbgPeak = Math.max(s.dbgPeak, rms);
    if (++s.dbgFrames >= 24) {
      console.debug(
        `[butler] peakRMS=${s.dbgPeak.toFixed(4)} gate=${gate.toFixed(4)} speaking=${s.speaking} busy=${s.busy}`,
      );
      s.dbgPeak = 0;
      s.dbgFrames = 0;
    }

    if (s.busy) {
      handleBusyFrame(s, d, rms, busyDeps);
      return;
    }
    if (s.micMuted) {
      resetCapture(s);
      return;
    }
    if (!s.speaking) {
      handleIdleFrame(s, d, rms, gate);
      return;
    }
    const utterance = handleCaptureFrame(s, d, rms, sustain);
    if (utterance) beginVoiceTurn(utterance);
  };

  const setMuted = (muted: boolean) => {
    s.micMuted = muted;
    cancelVerify();
    resetCapture(s);
    if (!muted) {
      // Relearn the room after unmuting; the device/background may have
      // changed while capture was disabled.
      recalibrateRoom(s);
    }
  };

  const submitText = (text: string) => {
    const typed = text.trim();
    if (typed === '') return;
    cancelVerify();
    stopTurn();
    resetCapture(s);
    s.busy = true;
    s.busyFrames = 0;
    s.replyAudible = false;
    spokenReply = '';
    abort = new AbortController();
    const myGen = ++s.turnGen;
    void runTextTurn(typed, playback, trackedSinks, abort.signal, s.playing, () => {
      if (myGen === s.turnGen) s.busyFrames = 0;
    }).finally(() => {
      if (myGen === s.turnGen) s.busy = false;
    });
  };

  const dispose = () => {
    if (s.disposed) return;
    s.disposed = true;
    cancelVerify();
    stopTurn();
    proc.onaudioprocess = null;
    try {
      source.disconnect(proc);
    } catch {
      // already disconnected by the enclosing audio-graph cleanup
    }
    try {
      proc.disconnect();
    } catch {
      // already disconnected
    }
  };

  return {
    processor: proc,
    submitText,
    setMuted,
    lastAudioFrameAt: () => s.lastFrameAt,
    dispose,
  };
}
