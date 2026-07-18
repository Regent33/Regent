// Pure VAD gate math (zero I/O) — see callLoop.ts for the loop that uses it.
// The tuned fixed threshold (0.015) suits a normal mic, but a quiet or heavily
// processed input (Acer PurifiedVoice / AI noise cancellation, a low input
// level) can peak BELOW it and hang forever on 'listening' — no turn ever
// starts. So gate on the running noise floor instead of a constant: in a QUIET
// room the gate drops toward a hard hiss floor so a soft mic still triggers a
// turn; in a NOISY room the floor is high and the gate stays at the original
// ceiling, so the hard-won noisy-room tuning is left exactly as it was.
export const VOICE_CEILING = 0.015; // the original fixed onset threshold = the ceiling
const ONSET_FLOOR = 0.006; // hard minimum onset gate — below this is indistinguishable from hiss
const OVER_FLOOR = 2.5; // in a noisy room, sit this far above the ambient floor
const SUSTAIN_RATIO = 0.6; // hysteresis: easier to STAY in speech than to enter it
const SUSTAIN_OVER_FLOOR = 1.2; // ...but sustain never drops below ambient, or noise reads as speech

/** Onset gate: cross this (from silence) to start a turn. Sensitivity is capped
 * at the tuned ceiling for a normal mic, but the gate is NEVER allowed below the
 * ambient floor itself — otherwise a loud room (floor above the ceiling) sits
 * above the gate and its own background noise reads as a turn. So past the
 * ceiling the gate tracks the room upward, keeping steady noise out of the call. */
export function voiceGate(noiseFloor: number): number {
  const capped = Math.min(VOICE_CEILING, Math.max(ONSET_FLOOR, noiseFloor * OVER_FLOOR));
  return Math.max(capped, noiseFloor * SUSTAIN_OVER_FLOOR);
}

/** Sustain gate: once speaking, stay voiced above this (lower than onset, but
 * always above the ambient floor so room tone never counts as voice). */
export function sustainGate(noiseFloor: number): number {
  return Math.max(voiceGate(noiseFloor) * SUSTAIN_RATIO, noiseFloor * SUSTAIN_OVER_FLOOR);
}

const BARGE_OVER_ONSET = 1.3; // barge-in sits just above onset — a false cut is costly
const INTERRUPT_OVER_FLOOR = 3.5; // and always this far above ambient — rejects TTS echo bleed

/** Barge-in gate: cross this to cut Regent off mid-reply. Built on the SAME
 * noise-floor-adaptive onset gate — a quiet / over-processed mic that can START
 * a turn (onset falls to 0.006) must also be able to INTERRUPT one. The old hard
 * 0.01 floor sat in the DEAD BAND above such a mic's speech: you could begin a
 * turn but never barge in. Now it tracks onset, nudged stricter (a false trigger
 * cuts speech), and still held above the ambient floor so Regent's own TTS bleed
 * can't self-trip it. `voiceGate`'s own 0.006 minimum keeps this off pure hiss. */
export function interruptGate(noiseFloor: number): number {
  return Math.max(voiceGate(noiseFloor) * BARGE_OVER_ONSET, noiseFloor * INTERRUPT_OVER_FLOOR);
}

/** True when several consecutive frame levels look like stationary room tone
 * rather than speech. Fan/air-conditioner/hiss RMS is almost flat over 200–300
 * ms; speech has a changing envelope. This is an energy-envelope check, not a
 * content classifier, and is used only after multiple frames have accumulated. */
export function isStationaryNoise(levels: readonly number[], maxVariation = 0.06): boolean {
  if (levels.length < 4) return false;
  const mean = levels.reduce((sum, level) => sum + level, 0) / levels.length;
  if (!(mean > 0)) return true;
  const variance = levels.reduce((sum, level) => sum + (level - mean) ** 2, 0) / levels.length;
  return Math.sqrt(variance) / mean <= maxVariation;
}

/** Cheap time-domain speech check for an onset frame. Broadband hiss/keyboard
 * edges cross zero and change sample-to-sample far more often than a human
 * voice, while low electrical/fan hum crosses too rarely. This is deliberately
 * only a second vote behind the adaptive energy gate—not a speech recognizer. */
export function isSpeechLikeFrame(samples: Float32Array, knownRms?: number): boolean {
  if (samples.length < 2) return false;
  let sumSquares = 0;
  let diffSquares = 0;
  let crossings = 0;
  let previous = samples[0];
  for (let index = 0; index < samples.length; index += 1) {
    const sample = samples[index];
    sumSquares += sample * sample;
    if (index > 0) {
      const diff = sample - previous;
      diffSquares += diff * diff;
      if ((sample >= 0) !== (previous >= 0)) crossings += 1;
    }
    previous = sample;
  }
  const rms = knownRms ?? Math.sqrt(sumSquares / samples.length);
  if (rms < 1e-5) return false;
  const zeroCrossingRate = crossings / (samples.length - 1);
  const normalizedDifference = Math.sqrt(diffSquares / (samples.length - 1)) / rms;
  return zeroCrossingRate >= 0.003 && zeroCrossingRate <= 0.34 && normalizedDifference <= 1.25;
}
