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

/** Onset gate: cross this (from silence) to start a turn. */
export function voiceGate(noiseFloor: number): number {
  return Math.min(VOICE_CEILING, Math.max(ONSET_FLOOR, noiseFloor * OVER_FLOOR));
}

/** Sustain gate: once speaking, stay voiced above this (lower than onset, but
 * always above the ambient floor so room tone never counts as voice). */
export function sustainGate(noiseFloor: number): number {
  return Math.max(voiceGate(noiseFloor) * SUSTAIN_RATIO, noiseFloor * SUSTAIN_OVER_FLOOR);
}
