// The turn-based local call — port of regent-web/hooks/localCall.ts: an
// energy-gated VAD loop that detects when the caller stops talking, POSTs the
// utterance to /call/turn, and plays the streamed reply (with barge-in).
// Tuning is local to the desktop capture path: short frames + onset confirmation
// reject noise without clipping the beginning of real speech.
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { voiceLanguageHint } from '@/shared/infrastructure/voice/protocol';
import type { CallPhase } from '@/features/butler/domain/phase';
import { type Playing, fetchCallToken, playPcm, wavBytes } from '@/features/butler/data/speechClient';
import {
  adaptNoiseFloor,
  confirmsSpeechWindow,
  interruptGate,
  isSpeechLikeFrame,
  isStationaryNonSpeech,
  isStationaryNoise,
  sustainGate,
  voiceGate,
} from '@/features/butler/domain/vad';

const PROCESSOR_SAMPLES = 2048; // ~43 ms at 48 kHz; halves endpoint granularity
const ONSET_WINDOW_FRAMES = 4; // ~170 ms rolling window; no blocking startup calibration
const ONSET_ACTIVE_FRAMES = 3; // one processed-audio dropout is tolerated
const PRE_ROLL_FRAMES = 8; // retain ~340 ms so onset confirmation never clips speech
const VAD_HANG = 10; // ~430 ms: keeps natural phrase pauses inside one utterance
const INTERRUPT_WINDOW_FRAMES = 5; // ~210 ms rolling barge-in window
const INTERRUPT_ACTIVE_FRAMES = 3; // deliberate speech in any 3/5 frames cuts TTS
const STATIONARY_TAIL_FRAMES = 6; // stable room tone must not prolong a turn
const MIN_VOICED_FRAMES = 4; // ~170 ms: reject a burst, retain short commands
const MAX_UTTERANCE_FRAMES = 600;
const BUSY_WATCHDOG_FRAMES = 1050; // ~45s of true silence ends a hung turn
const average = (levels: readonly number[]) => levels.reduce((sum, level) => sum + level, 0) / levels.length;

export interface CallSinks {
  setPhase: (p: CallPhase) => void;
  setHeard: (s: string) => void;
  setReply: (s: string) => void;
  setError: (s: string | null) => void;
  /** Holds the first voice chunk until a diagram is visibly ready. */
  waitForVisual: () => Promise<void>;
  /** Resolves an inline/artifact/fallback visual after the reply stream ends. */
  finalizeVisual: () => Promise<void>;
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

export interface CallLoopController {
  readonly processor: ScriptProcessorNode;
  readonly submitText: (text: string) => void;
  readonly setMuted: (muted: boolean) => void;
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
): CallLoopController {
  // ponytail: ScriptProcessorNode is deprecated but works everywhere and needs
  // no separate worklet file; swap for an AudioWorklet if it ever drops.
  const proc = ctx.createScriptProcessor(PROCESSOR_SAMPLES, 1, 1);
  source.connect(proc);
  proc.connect(ctx.destination); // keeps the node pulling; it outputs silence

  let buf: Float32Array[] = [];
  let speaking = false;
  let silence = 0;
  let onsetLevels: number[] = [];
  let onsetSpeechLike: boolean[] = [];
  let preRoll: Float32Array[] = [];
  let busy = false;
  let replyAudible = false; // has Regent's TTS begun this turn? gates barge-in vs. the thinking phase
  let interruptLevels: number[] = [];
  let interruptSpeechLike: boolean[] = [];
  let interruptBuf: Float32Array[] = [];
  let turnGen = 0; // only the latest turn's completion may clear `busy`
  let abort: AbortController | null = null;
  const playing: Playing = { src: null };
  let noiseFloor = 0;
  let sustainLevels: number[] = [];
  let sustainSpeechLike: boolean[] = [];
  let voiced = 0;
  let busyFrames = 0;
  // Diagnostics (~1/sec): if peakRMS stays ~0 while you talk, audio isn't
  // reaching the loop (suspended context / wrong device); below thr, onset
  // can't fire (mic too quiet). Same instrument the source file shipped with.
  let dbgPeak = 0;
  let dbgFrames = 0;
  let micMuted = false;
  console.debug(`[butler] VAD loop started (ctx.state=${ctx.state})`);

  const resetInterrupt = () => {
    interruptLevels = [];
    interruptSpeechLike = [];
    interruptBuf = [];
  };

  const resetCapture = () => {
    buf = [];
    speaking = false;
    silence = 0;
    onsetLevels = [];
    onsetSpeechLike = [];
    preRoll = [];
    resetInterrupt();
    sustainLevels = [];
    sustainSpeechLike = [];
    voiced = 0;
  };

  const recalibrateRoom = () => {
    resetCapture();
    noiseFloor = 0;
  };

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
    // Gates use room tone learned while idle. Never learn while `busy`:
    // residual TTS echo otherwise raises the floor throughout a long reply and
    // eventually puts barge-in above the caller's microphone level.
    const gate = voiceGate(noiseFloor);
    const sustain = sustainGate(noiseFloor);

    dbgPeak = Math.max(dbgPeak, rms);
    if (++dbgFrames >= 24) {
      console.debug(
        `[butler] peakRMS=${dbgPeak.toFixed(4)} gate=${gate.toFixed(4)} speaking=${speaking} busy=${busy}`,
      );
      dbgPeak = 0;
      dbgFrames = 0;
    }

    if (busy) {
      if (playing.src) {
        busyFrames = 0; // audible reply = the turn is alive
        replyAudible = true; // ...and Regent has now started speaking this turn
      }
      if (++busyFrames > BUSY_WATCHDOG_FRAMES) {
        stopTurn();
        turnGen += 1;
        busy = false;
        busyFrames = 0;
        resetCapture();
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
      if (!replyAudible) {
        resetInterrupt();
        return;
      }
      // Barge-in: gated above the ambient floor so noise never cuts Regent off,
      // but adaptive to a quiet mic (same onset math) so a soft voice can still
      // interrupt — no hard floor stranding a quiet mic that can start a turn.
      if (micMuted) {
        resetInterrupt();
        return;
      }
      // Rolling confirmation tolerates a single WebRTC/AEC dropout. The old
      // consecutive counter erased the attempt on any dip and routinely never
      // reached its ~430 ms target while Regent's playback DSP was active.
      const bargeGate = interruptGate(noiseFloor);
      interruptLevels.push(rms);
      interruptSpeechLike.push(isSpeechLikeFrame(d, rms));
      interruptBuf.push(new Float32Array(d));
      if (interruptLevels.length > INTERRUPT_WINDOW_FRAMES) {
        interruptLevels.shift();
        interruptSpeechLike.shift();
        interruptBuf.shift();
      }
      if (
        confirmsSpeechWindow(
          interruptLevels,
          interruptSpeechLike,
          bargeGate,
          INTERRUPT_ACTIVE_FRAMES,
        )
      ) {
        const captured = interruptBuf;
        const active = interruptLevels.filter((level) => level > bargeGate).length;
        stopTurn();
        turnGen += 1;
        busy = false;
        speaking = true;
        silence = 0;
        resetInterrupt();
        voiced = active;
        buf = captured;
        sinks.setPhase('listening');
      }
      return;
    }

    if (micMuted) {
      resetCapture();
      return;
    }

    if (!speaking) {
      // Learn quiet room tone immediately; a louder stationary bed is absorbed
      // below after one short window. There is no unconditional half-second
      // calibration pause, so speech at Butler startup is admitted at once.
      if (rms <= gate) noiseFloor = adaptNoiseFloor(noiseFloor, rms, true);
      preRoll.push(new Float32Array(d));
      if (preRoll.length > PRE_ROLL_FRAMES) preRoll.shift();
      onsetLevels.push(rms);
      onsetSpeechLike.push(isSpeechLikeFrame(d, rms));
      if (onsetLevels.length > ONSET_WINDOW_FRAMES) {
        onsetLevels.shift();
        onsetSpeechLike.shift();
      }
      if (confirmsSpeechWindow(onsetLevels, onsetSpeechLike, gate, ONSET_ACTIVE_FRAMES)) {
        speaking = true;
        voiced = onsetLevels.filter((level) => level > gate).length;
        silence = 0;
        buf = preRoll;
        preRoll = [];
        onsetLevels = [];
        onsetSpeechLike = [];
      } else if (
        onsetLevels.length === ONSET_WINDOW_FRAMES &&
        isStationaryNoise(onsetLevels) &&
        onsetLevels.filter((level) => level > gate).length >= ONSET_ACTIVE_FRAMES
      ) {
        // A fan/traffic bed has energy but failed the speech vote. Absorb it
        // quickly so the rolling window does not keep reconsidering it.
        noiseFloor = Math.max(noiseFloor, average(onsetLevels));
        onsetLevels = [];
        onsetSpeechLike = [];
      }
      return;
    }

    buf.push(new Float32Array(d));
    if (rms > sustain) {
      sustainLevels.push(rms);
      sustainSpeechLike.push(isSpeechLikeFrame(d, rms));
      if (sustainLevels.length > STATIONARY_TAIL_FRAMES) {
        sustainLevels.shift();
        sustainSpeechLike.shift();
      }
      if (
        sustainLevels.length >= STATIONARY_TAIL_FRAMES &&
        isStationaryNonSpeech(sustainLevels, sustainSpeechLike)
      ) {
        // Real speech ended into a steady non-speech fan/road bed. The stable
        // frames already observed count toward the endpoint, but no 250 ms
        // energy-only shortcut may clip a level-compressed vowel or sentence.
        noiseFloor = Math.max(noiseFloor, average(sustainLevels));
        silence = Math.max(silence + 1, STATIONARY_TAIL_FRAMES);
      } else {
        voiced += 1;
        silence = 0;
      }
    } else {
      sustainLevels = [];
      sustainSpeechLike = [];
      noiseFloor = adaptNoiseFloor(noiseFloor, rms, true);
      silence += 1;
    }
    if (silence >= VAD_HANG || buf.length > MAX_UTTERANCE_FRAMES) {
      speaking = false;
      silence = 0;
      const utterance = buf;
      buf = [];
      sustainLevels = [];
      sustainSpeechLike = [];
      if (voiced < MIN_VOICED_FRAMES) return; // noise blip — drop, stay listening
      resetInterrupt();
      busy = true;
      busyFrames = 0;
      replyAudible = false;
      abort = new AbortController();
      const myGen = ++turnGen;
      void runTurn(utterance, ctx.sampleRate, playback, sinks, abort.signal, playing, () => {
        if (myGen === turnGen) busyFrames = 0; // streamed line → watchdog reset
      })
        .then((outcome) => {
          // Server said noise — KEEP the learned floor. Zeroing it here (the
          // old recalibrateRoom) dropped the gate to its hair-trigger minimum
          // right after the room just proved noisy, so the same background
          // re-triggered instantly (the noise → reject → noise loop). The
          // idle EMA still re-lowers the floor if the room actually quiets.
          if (myGen === turnGen && outcome === 'noise') resetCapture();
        })
        .finally(() => {
          if (myGen === turnGen) busy = false; // ignore a turn we barged over
        });
    }
  };

  const setMuted = (muted: boolean) => {
    micMuted = muted;
    resetCapture();
    if (!muted) {
      // Relearn the room after unmuting; the device/background may have
      // changed while capture was disabled.
      recalibrateRoom();
    }
  };

  const submitText = (text: string) => {
    const typed = text.trim();
    if (typed === '') return;
    stopTurn();
    resetCapture();
    busy = true;
    busyFrames = 0;
    replyAudible = false;
    abort = new AbortController();
    const myGen = ++turnGen;
    void runTextTurn(typed, playback, sinks, abort.signal, playing, () => {
      if (myGen === turnGen) busyFrames = 0;
    }).finally(() => {
      if (myGen === turnGen) busy = false;
    });
  };

  return { processor: proc, submitText, setMuted };
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
): Promise<TurnOutcome> {
  sinks.setPhase('thinking');
  let res: Response;
  try {
    const url = new URL(`${SPEECH_URL}/call/turn`);
    const language = voiceLanguageHint(
      typeof navigator === 'undefined' ? undefined : navigator.language,
    );
    if (language) url.searchParams.set('language', language);
    res = await fetch(url, {
      method: 'POST',
      body: wavBytes(frames, sampleRate),
      headers: { 'x-call-token': await fetchCallToken() },
      signal,
    });
  } catch (e) {
    if ((e as Error).name === 'AbortError') return 'normal'; // barged over
    sinks.setError('Speech server unreachable — reopen Butler Mode to restart it.');
    sinks.setPhase('listening');
    return 'normal';
  }
  return consumeTurnResponse(res, playback, sinks, signal, playing, onProgress);
}

/** Typed turn: same streamed response and voice path, without WAV/ASR. */
async function runTextTurn(
  text: string,
  playback: PlaybackSink,
  sinks: CallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<TurnOutcome> {
  sinks.setPhase('thinking');
  let res: Response;
  try {
    res = await fetch(`${SPEECH_URL}/call/text`, {
      method: 'POST',
      body: text,
      headers: {
        'content-type': 'text/plain; charset=utf-8',
        'x-call-token': await fetchCallToken(),
      },
      signal,
    });
  } catch (e) {
    if ((e as Error).name === 'AbortError') return 'normal';
    sinks.setError('Speech server unreachable — reopen Butler Mode to restart it.');
    sinks.setPhase('listening');
    return 'normal';
  }
  return consumeTurnResponse(res, playback, sinks, signal, playing, onProgress);
}

type TurnOutcome = 'normal' | 'noise';

async function consumeTurnResponse(
  res: Response,
  playback: PlaybackSink,
  sinks: CallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<TurnOutcome> {
  if (res.status === 401 || res.status === 403) {
    // Running server without our CORS/token grant (started by the CLI).
    sinks.setError(
      'The voice server is running without desktop access. Stop it (Stop-Process -Name regent-voice-server) and reopen Butler Mode.',
    );
    sinks.setPhase('listening');
    return 'normal';
  }
  if (!res.ok) {
    const failure = (await res.json().catch(() => ({}))) as { error?: string };
    sinks.setError(failure.error || `Speech server rejected the turn (${res.status}).`);
    sinks.setPhase('listening');
    return 'normal';
  }
  if (!res.body) {
    sinks.setPhase('listening');
    return 'normal';
  }
  sinks.setError(null);

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let acc = '';
  let spokeYet = false;
  let noiseRejected = false;
  // Keep reading while ordered audio plays. This is essential when a filler
  // arrives before the completed diagram spec: the later reply can still be
  // consumed and render/release the visual instead of deadlocking the stream.
  let audioQueue = Promise.resolve();
  try {
    for (;;) {
      if (signal.aborted) return 'normal';
      const { done, value } = await reader.read();
      if (done) break;
      acc += decoder.decode(value, { stream: true });
      let nl: number;
      while ((nl = acc.indexOf('\n')) >= 0) {
        const line = acc.slice(0, nl);
        acc = acc.slice(nl + 1);
        if (!line.trim()) continue;
        let msg: {
          heard?: string;
          reply?: string;
          audio?: string;
          error?: string;
          noise?: { reason?: string } | string;
        };
        try {
          msg = JSON.parse(line);
        } catch {
          continue;
        }
        onProgress(); // any valid line (incl. keepalives) proves the turn is alive
        // Empty ASR is an ambient/noise miss, not something the caller said.
        if (typeof msg.heard === 'string' && msg.heard.trim() !== '') sinks.setHeard(msg.heard);
        if (typeof msg.reply === 'string') sinks.setReply(msg.reply);
        if (msg.error) sinks.setError(msg.error);
        if (msg.noise) noiseRejected = true;
        if (typeof msg.audio === 'string') {
          const audio = msg.audio;
          const first = !spokeYet;
          spokeYet = true;
          audioQueue = audioQueue.then(async () => {
            if (signal.aborted) return;
            if (first) {
              await sinks.waitForVisual();
              if (signal.aborted) return;
              sinks.setPhase('speaking');
            }
            await playPcm(playback.ctx, playback.node, audio, signal, playing);
          });
        }
      }
    }
  } catch {
    return 'normal'; // aborted reader read → swallow
  }
  if (noiseRejected) {
    if (!signal.aborted) sinks.setPhase('listening');
    return 'noise';
  }
  await sinks.finalizeVisual();
  await audioQueue.catch(() => undefined);
  if (!signal.aborted) sinks.setPhase('listening');
  return 'normal';
}
