// The `busy` branch of the VAD loop: watchdog for hung turns and barge-in
// detection while Regent is replying. Mutates the shared LoopState.
import { confirmsSpeechWindow, interruptGate, isSpeechLikeFrame } from '@/features/butler/domain/vad';
import type { CallSinks, PlaybackSink } from '@/features/butler/data/callTypes';
import {
  BUSY_WATCHDOG_FRAMES,
  ECHO_REARM_QUIET_FRAMES,
  INTERRUPT_ACTIVE_FRAMES,
  INTERRUPT_WINDOW_FRAMES,
  type LoopState,
  USER_LEVEL_RATIO,
  resetCapture,
  resetInterrupt,
} from '@/features/butler/data/loopState';

export interface BusyDeps {
  playback: PlaybackSink;
  sinks: CallSinks;
  stopTurn: () => void;
}

export function handleBusyFrame(s: LoopState, d: Float32Array, rms: number, deps: BusyDeps): void {
  const { playback, sinks, stopTurn } = deps;
  if (s.playing.src) {
    s.busyFrames = 0; // audible reply = the turn is alive
    if (!s.replyAudible) {
      s.replyAudible = true; // ...and Regent has now started speaking this turn
      s.echo.startReply(); // begin the echo-learn warmup + a fresh peak-hold
    } else if (s.playQuiet > ECHO_REARM_QUIET_FRAMES) {
      // Speech resumed after a REAL gap (the ungated filler → a long think →
      // the first answer sentence). The warmup was consumed by the filler and
      // the learned coupling may be frozen low (post-warmup learning reads
      // the louder answer onset as double-talk) — the recipe for Regent
      // barging over himself. Re-arm so this onset learns unconditionally
      // again. Sentence boundaries never trip this: their gaps sit inside
      // the estimator's ~150ms peak-hold release.
      s.echo.startReply();
    }
    s.playQuiet = 0;
  } else if (s.replyAudible) {
    s.playQuiet += 1;
  }
  if (++s.busyFrames > BUSY_WATCHDOG_FRAMES) {
    stopTurn();
    s.turnGen += 1;
    s.busy = false;
    s.busyFrames = 0;
    resetCapture(s);
    sinks.setPhase('listening');
    sinks.setError('That took too long — I reset. Try again.');
    return;
  }
  // Barge-in only makes sense once Regent is audibly replying. Before the
  // first audio clip (still THINKING) mic energy is the room, not an
  // interruption — discard it so noise can't abort a legitimate request.
  // But the server streams ONE clip per sentence, so playing.src goes
  // briefly null in the GAPS between sentences; the old `!playing.src`
  // reset fired in those gaps and kept barge-in from ever accumulating
  // across a sentence boundary (the "can't interrupt Regent" bug).
  // `replyAudible` stays true through the gaps — no TTS plays in them, so
  // measuring the mic there is clean — so talking over Regent now cuts in.
  if (!s.replyAudible) {
    resetInterrupt(s);
    return;
  }
  // Barge-in: gated above the ambient floor so noise never cuts Regent off,
  // but adaptive to a quiet mic (same onset math) so a soft voice can still
  // interrupt — no hard floor stranding a quiet mic that can start a turn.
  if (s.micMuted) {
    resetInterrupt(s);
    return;
  }
  // Rolling confirmation tolerates a single WebRTC/AEC dropout. The old
  // consecutive counter erased the attempt on any dip and routinely never
  // reached its ~430 ms target while Regent's playback DSP was active.
  // The gate also demands a level in the ballpark of the CALLER'S OWN voice
  // (learned during their turns): birdsong through a window cleared the
  // floor-relative gate and cut Regent mid-reply, but no ambient bed reaches
  // how the caller actually sounds at their own mic.
  const bargeGate = Math.max(interruptGate(s.noiseFloor), s.userLevel * USER_LEVEL_RATIO);
  // Compensate the barge ENERGY for Regent's own TTS echo; the shape vote and
  // the ASR buffer stay on the RAW frame — echo IS speech-shaped, so only the
  // compensated energy gate can reject it. This is what stops the self-barge.
  playback.node.getFloatTimeDomainData(s.echoScratch);
  let playSum = 0;
  for (let i = 0; i < s.echoScratch.length; i++) playSum += s.echoScratch[i] * s.echoScratch[i];
  const compensated = s.echo.compensate(rms, Math.sqrt(playSum / s.echoScratch.length));
  s.interruptLevels.push(compensated);
  s.interruptSpeechLike.push(isSpeechLikeFrame(d, rms));
  s.interruptBuf.push(new Float32Array(d));
  if (s.interruptLevels.length > INTERRUPT_WINDOW_FRAMES) {
    s.interruptLevels.shift();
    s.interruptSpeechLike.shift();
    s.interruptBuf.shift();
  }
  if (
    confirmsSpeechWindow(
      s.interruptLevels,
      s.interruptSpeechLike,
      bargeGate,
      INTERRUPT_ACTIVE_FRAMES,
    ) &&
    // Final veto: echo passes the shape vote (it IS speech) and, whenever the
    // gain-based estimator lags a nonstationary mic (AGC / noise suppression
    // reshaping gain mid-sentence), the energy gate too. But echo TRACKS the
    // render envelope and a real caller doesn't — so a mic that correlates
    // with playback is Regent hearing himself, never an interruption.
    !s.echo.echoLikely()
  ) {
    const captured = s.interruptBuf;
    const active = s.interruptLevels.filter((level) => level > bargeGate).length;
    stopTurn();
    s.turnGen += 1;
    s.busy = false;
    s.speaking = true;
    s.silence = 0;
    resetInterrupt(s);
    s.voiced = active;
    s.buf = captured;
    sinks.setPhase('listening');
  }
}
