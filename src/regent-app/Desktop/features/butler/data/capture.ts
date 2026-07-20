// Idle-onset detection and utterance capture/endpointing for the VAD loop.
// Mutates the shared LoopState; callLoop.ts starts the turn when
// handleCaptureFrame returns a finished utterance.
import {
  adaptNoiseFloor,
  confirmsSpeechWindow,
  isSpeechLikeFrame,
  isStationaryNonSpeech,
  isStationaryNoise,
  shouldEndUtterance,
} from '@/features/butler/domain/vad';
import {
  type LoopState,
  MIN_VOICED_FRAMES,
  ONSET_ACTIVE_FRAMES,
  ONSET_WINDOW_FRAMES,
  PRE_ROLL_FRAMES,
  STATIONARY_TAIL_FRAMES,
  average,
} from '@/features/butler/data/loopState';

/** Not speaking yet: learn room tone, keep a pre-roll, confirm speech onset. */
export function handleIdleFrame(s: LoopState, d: Float32Array, rms: number, gate: number): void {
  // Learn quiet room tone immediately; a louder stationary bed is absorbed
  // below after one short window. There is no unconditional half-second
  // calibration pause, so speech at Butler startup is admitted at once.
  if (rms <= gate) s.noiseFloor = adaptNoiseFloor(s.noiseFloor, rms, true);
  s.preRoll.push(new Float32Array(d));
  if (s.preRoll.length > PRE_ROLL_FRAMES) s.preRoll.shift();
  s.onsetLevels.push(rms);
  s.onsetSpeechLike.push(isSpeechLikeFrame(d, rms));
  if (s.onsetLevels.length > ONSET_WINDOW_FRAMES) {
    s.onsetLevels.shift();
    s.onsetSpeechLike.shift();
  }
  if (confirmsSpeechWindow(s.onsetLevels, s.onsetSpeechLike, gate, ONSET_ACTIVE_FRAMES)) {
    s.speaking = true;
    s.voiced = s.onsetLevels.filter((level) => level > gate).length;
    s.silence = 0;
    s.buf = s.preRoll;
    s.preRoll = [];
    s.onsetLevels = [];
    s.onsetSpeechLike = [];
  } else if (
    s.onsetLevels.length === ONSET_WINDOW_FRAMES &&
    isStationaryNoise(s.onsetLevels) &&
    s.onsetLevels.filter((level) => level > gate).length >= ONSET_ACTIVE_FRAMES
  ) {
    // A fan/traffic bed has energy but failed the speech vote. Absorb it
    // quickly so the rolling window does not keep reconsidering it.
    s.noiseFloor = Math.max(s.noiseFloor, average(s.onsetLevels));
    s.onsetLevels = [];
    s.onsetSpeechLike = [];
  }
}

/**
 * Speaking: accumulate the utterance and detect its endpoint. Returns the
 * captured frames when a valid utterance ended, null otherwise (still
 * speaking, or a noise blip that was dropped).
 */
export function handleCaptureFrame(
  s: LoopState,
  d: Float32Array,
  rms: number,
  sustain: number,
): Float32Array[] | null {
  s.buf.push(new Float32Array(d));
  if (rms > sustain) {
    const speechLike = isSpeechLikeFrame(d, rms);
    s.sustainLevels.push(rms);
    s.sustainSpeechLike.push(speechLike);
    if (s.sustainLevels.length > STATIONARY_TAIL_FRAMES) {
      s.sustainLevels.shift();
      s.sustainSpeechLike.shift();
    }
    if (
      s.sustainLevels.length >= STATIONARY_TAIL_FRAMES &&
      isStationaryNonSpeech(s.sustainLevels, s.sustainSpeechLike)
    ) {
      // Real speech ended into a steady non-speech fan/road bed. The stable
      // frames already observed count toward the endpoint, but no 250 ms
      // energy-only shortcut may clip a level-compressed vowel or sentence.
      s.noiseFloor = Math.max(s.noiseFloor, average(s.sustainLevels));
      s.silence = Math.max(s.silence + 1, STATIONARY_TAIL_FRAMES);
    } else if (speechLike) {
      s.voiced += 1;
      s.silence = 0;
      // Track how loud the CALLER sounds at this mic; barge-in later demands
      // a level in this ballpark, which distant ambient beds never reach.
      s.userLevel = s.userLevel === 0 ? rms : s.userLevel * 0.9 + rms * 0.1;
    } else {
      // Energy WITHOUT speech shape (typing, clatter, dish clinks). Before,
      // any above-gate frame reset the endpoint, so a lively room held
      // capture open to the 12s ceiling — the "stalls on Listening" report —
      // and then padded the utterance ASR sees with noise. Count it toward
      // the endpoint instead; a word's own fricatives last only a few frames
      // before a voiced frame resets the counter, so speech is unaffected.
      s.silence += 1;
    }
  } else {
    s.sustainLevels = [];
    s.sustainSpeechLike = [];
    s.noiseFloor = adaptNoiseFloor(s.noiseFloor, rms, true);
    s.silence += 1;
  }
  if (!shouldEndUtterance(s.silence, s.buf.length)) return null;
  s.speaking = false;
  s.silence = 0;
  const utterance = s.buf;
  s.buf = [];
  s.sustainLevels = [];
  s.sustainSpeechLike = [];
  if (s.voiced < MIN_VOICED_FRAMES) return null; // noise blip — drop, stay listening
  return utterance;
}
