// The turn-based local call — port of regent-web/hooks/localCall.ts: an
// energy-gated VAD loop that detects when the caller stops talking, POSTs the
// utterance to /call/turn, and plays the streamed reply (with barge-in).
// Tuning constants and watchdog semantics are IDENTICAL to the source — they
// encode months of call debugging (see its comments); change them there first.
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { openMicPrivacySettings } from '@/shared/infrastructure/opener';
import type { CallPhase } from '@/features/butler/domain/phase';
import { type Playing, fetchCallToken, playPcm, wavBytes } from '@/features/butler/data/speechClient';
import { VOICE_CEILING, sustainGate, voiceGate } from '@/features/butler/domain/vad';

const VAD_HANG = 6;
const INTERRUPT_THRESHOLD = 0.02;
const INTERRUPT_FRAMES = 3;
const FLOOR_RISE = 0.01;
const FLOOR_FALL = 0.15;
const INTERRUPT_OVER_FLOOR = 3.5;
const MIN_VOICED_FRAMES = 2;
const MAX_UTTERANCE_FRAMES = 300;
const BUSY_WATCHDOG_FRAMES = 235; // ~20s of true silence ends a hung turn

export interface CallSinks {
  setPhase: (p: CallPhase) => void;
  setHeard: (s: string) => void;
  setReply: (s: string) => void;
  setError: (s: string | null) => void;
}

/** Where Regent's reply audio renders: a SEPARATE AudioContext from the mic's.
 * The mic context carries an echo-cancelled capture, which Windows opens as a
 * communications session — rendering through that same context put the reply
 * (and, via driver voice-call DSP, every other app's audio) onto the
 * phone-call processing path: muffled TTS, ducked music. A capture-free
 * context renders as plain media at full quality. */
export interface PlaybackSink {
  ctx: AudioContext;
  node: AnalyserNode;
}

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
): ScriptProcessorNode {
  // ponytail: ScriptProcessorNode is deprecated but works everywhere and needs
  // no separate worklet file; swap for an AudioWorklet if it ever drops.
  const proc = ctx.createScriptProcessor(4096, 1, 1);
  source.connect(proc);
  proc.connect(ctx.destination); // keeps the node pulling; it outputs silence

  let buf: Float32Array[] = [];
  let speaking = false;
  let silence = 0;
  let busy = false;
  let interruptFrames = 0;
  let turnGen = 0; // only the latest turn's completion may clear `busy`
  let abort: AbortController | null = null;
  const playing: Playing = { src: null };
  let noiseFloor = 0;
  let voiced = 0;
  let everVoiced = false; // did any onset ever fire? (gates the quiet-mic warning)
  let busyFrames = 0;
  // Diagnostics (~1/sec): if peakRMS stays ~0 while you talk, audio isn't
  // reaching the loop (suspended context / wrong device); below thr, onset
  // can't fire (mic too quiet). Same instrument the source file shipped with.
  let dbgPeak = 0;
  let dbgFrames = 0;
  // Silent-mic watchdog: ~10s of essentially-zero input means audio is not
  // reaching the loop (wrong device / muted mic) — say so once, visibly.
  let lifetimeFrames = 0;
  let lifetimePeak = 0;
  let warnedSilent = false;
  console.debug(`[butler] VAD loop started (ctx.state=${ctx.state})`);

  const stopTurn = () => {
    abort?.abort();
    abort = null;
    if (playing.src) {
      try {
        playing.src.stop(); // fires onended → resolves the awaiting playPcm
      } catch {
        // already stopped
      }
      playing.src = null;
    }
  };

  proc.onaudioprocess = (e) => {
    const d = e.inputBuffer.getChannelData(0);
    let sum = 0;
    for (let i = 0; i < d.length; i++) sum += d[i] * d[i];
    const rms = Math.sqrt(sum / d.length);
    const a = rms > noiseFloor ? FLOOR_RISE : FLOOR_FALL;
    noiseFloor = noiseFloor * (1 - a) + rms * a;
    // Onset/sustain gates adapt to the noise floor so a quiet or over-processed
    // mic still triggers (see vad.ts); barge-in keeps its own fixed thresholds.
    const gate = voiceGate(noiseFloor);
    const sustain = sustainGate(noiseFloor);

    lifetimePeak = Math.max(lifetimePeak, rms);
    // ~10s in, if NO turn has ever started, say why. <0.004 = essentially no
    // signal (wrong/blocked device) → open the privacy page. 0.004–ceiling =
    // signal present but too weak to gate: a low input level or an
    // over-processing app (Acer PurifiedVoice) — tell the user how to fix it.
    if (++lifetimeFrames === 118 && !warnedSilent && !everVoiced && lifetimePeak < VOICE_CEILING) {
      warnedSilent = true;
      if (lifetimePeak < 0.004) {
        // Windows can't re-summon its permission popup on demand, so open the
        // mic privacy page directly instead of describing the settings path.
        openMicPrivacySettings();
        sinks.setError(
          'No microphone signal — opened Windows mic settings for you. Allow desktop apps, pick the right input device in Sound settings, then reopen Butler Mode.',
        );
      } else {
        sinks.setError(
          'Your mic signal is very low. Turn up the input level in Sound settings, or disable Acer PurifiedVoice / AI noise cancellation, then reopen Butler Mode.',
        );
      }
    }

    dbgPeak = Math.max(dbgPeak, rms);
    if (++dbgFrames >= 12) {
      console.debug(
        `[butler] peakRMS=${dbgPeak.toFixed(4)} gate=${gate.toFixed(4)} speaking=${speaking} busy=${busy}`,
      );
      dbgPeak = 0;
      dbgFrames = 0;
    }

    if (busy) {
      if (playing.src) busyFrames = 0; // audible reply = the turn is alive
      if (++busyFrames > BUSY_WATCHDOG_FRAMES) {
        stopTurn();
        turnGen += 1;
        busy = false;
        busyFrames = 0;
        speaking = false;
        interruptFrames = 0;
        buf = [];
        sinks.setPhase('listening');
        sinks.setError('That took too long — I reset. Try again.');
        return;
      }
      // Barge-in: gated above the ambient floor so noise never cuts Regent off.
      if (rms > Math.max(INTERRUPT_THRESHOLD, noiseFloor * INTERRUPT_OVER_FLOOR)) {
        interruptFrames += 1;
        if (interruptFrames > INTERRUPT_FRAMES) {
          stopTurn();
          turnGen += 1;
          busy = false;
          speaking = true;
          silence = 0;
          interruptFrames = 0;
          voiced = 1;
          buf = [new Float32Array(d)];
          sinks.setPhase('listening');
        }
      } else {
        interruptFrames = 0;
      }
      return;
    }

    if (!speaking) {
      // Onset on the adaptive gate — your voice always starts a turn, even on a
      // quiet mic (gate drops with the noise floor; see vad.ts).
      if (rms > gate) {
        speaking = true;
        everVoiced = true;
        voiced = 1;
        silence = 0;
        buf.push(new Float32Array(d));
      }
      return;
    }

    buf.push(new Float32Array(d));
    if (rms > sustain) {
      voiced += 1;
      silence = 0;
    } else {
      silence += 1;
    }
    if (silence > VAD_HANG || buf.length > MAX_UTTERANCE_FRAMES) {
      speaking = false;
      silence = 0;
      const utterance = buf;
      buf = [];
      if (voiced < MIN_VOICED_FRAMES) return; // noise blip — drop, stay listening
      busy = true;
      busyFrames = 0;
      interruptFrames = 0;
      abort = new AbortController();
      const myGen = ++turnGen;
      void runTurn(utterance, ctx.sampleRate, playback, sinks, abort.signal, playing, () => {
        if (myGen === turnGen) busyFrames = 0; // streamed line → watchdog reset
      }).finally(() => {
        if (myGen === turnGen) busy = false; // ignore a turn we barged over
      });
    }
  };

  return proc;
}

/** One turn: WAV-encode the utterance, stream /call/turn, play each chunk. */
async function runTurn(
  frames: Float32Array[],
  sampleRate: number,
  playback: PlaybackSink,
  sinks: CallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<void> {
  sinks.setPhase('thinking');
  let res: Response;
  try {
    res = await fetch(`${SPEECH_URL}/call/turn?language=English&speed=1`, {
      method: 'POST',
      body: wavBytes(frames, sampleRate),
      headers: { 'x-call-token': await fetchCallToken() },
      signal,
    });
  } catch (e) {
    if ((e as Error).name === 'AbortError') return; // barged over
    sinks.setError('Speech server unreachable — reopen Butler Mode to restart it.');
    sinks.setPhase('listening');
    return;
  }
  if (res.status === 401 || res.status === 403) {
    // Running server without our CORS/token grant (started by the CLI).
    sinks.setError(
      'The voice server is running without desktop access. Stop it (Stop-Process -Name regent-voice-server) and reopen Butler Mode.',
    );
    sinks.setPhase('listening');
    return;
  }
  if (!res.body) {
    sinks.setPhase('listening');
    return;
  }
  sinks.setError(null);

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let acc = '';
  let spokeYet = false;
  try {
    for (;;) {
      if (signal.aborted) return;
      const { done, value } = await reader.read();
      if (done) break;
      acc += decoder.decode(value, { stream: true });
      let nl: number;
      while ((nl = acc.indexOf('\n')) >= 0) {
        const line = acc.slice(0, nl);
        acc = acc.slice(nl + 1);
        if (!line.trim()) continue;
        let msg: { heard?: string; reply?: string; audio?: string; error?: string };
        try {
          msg = JSON.parse(line);
        } catch {
          continue;
        }
        onProgress(); // any valid line (incl. keepalives) proves the turn is alive
        if (typeof msg.heard === 'string') sinks.setHeard(msg.heard || "(didn't catch that)");
        if (typeof msg.reply === 'string') sinks.setReply(msg.reply);
        if (msg.error) sinks.setError(msg.error);
        if (typeof msg.audio === 'string') {
          if (signal.aborted) return;
          if (!spokeYet) {
            spokeYet = true;
            sinks.setPhase('speaking');
          }
          await playPcm(playback.ctx, playback.node, msg.audio, signal, playing); // sequential → gapless
        }
      }
    }
  } catch {
    return; // aborted reader read → swallow
  }
  if (!signal.aborted) sinks.setPhase('listening');
}
