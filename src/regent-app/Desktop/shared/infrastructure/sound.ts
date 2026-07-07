// WebAudio chime — no binary asset. A short two-note "ding" synthesized with
// OscillatorNode, used to signal a background turn finishing. Runs in the
// webview only; safe to call from anywhere (no-ops without AudioContext).
const NOTES: readonly [freq: number, start: number][] = [
  [660, 0], // E5
  [880, 0.075], // A5, a beat later — reads as a soft two-note "ding"
];
const NOTE_DURATION = 0.09;

/** Plays the ~150ms completion chime. Each note ramps in and back out
 * (exponential gain) so it never clicks or buzzes. */
export function playChime(): void {
  if (typeof window === 'undefined') return;
  const AudioCtx = window.AudioContext;
  if (AudioCtx === undefined) return;
  const ctx = new AudioCtx();
  for (const [freq, start] of NOTES) {
    const osc = ctx.createOscillator();
    const gain = ctx.createGain();
    osc.type = 'sine';
    osc.frequency.value = freq;
    const t0 = ctx.currentTime + start;
    gain.gain.setValueAtTime(0.0001, t0);
    gain.gain.exponentialRampToValueAtTime(0.2, t0 + 0.01);
    gain.gain.exponentialRampToValueAtTime(0.0001, t0 + NOTE_DURATION);
    osc.connect(gain).connect(ctx.destination);
    osc.start(t0);
    osc.stop(t0 + NOTE_DURATION + 0.01);
  }
  // Give the last note time to finish, then release the context — nothing
  // keeps it alive between chimes.
  window.setTimeout(() => void ctx.close(), 250);
}
