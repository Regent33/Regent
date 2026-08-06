import { describe, expect, test } from 'bun:test';
import { handleCaptureFrame } from './capture';
import { createLoopState, type LoopState } from './loopState';

// Above-gate energy with NO speech shape: alternating full-scale samples give
// a zero-crossing rate of 1.0, far past the speech vote's 0.34 ceiling.
function noiseFrame(): Float32Array {
  const d = new Float32Array(512);
  for (let i = 0; i < d.length; i++) d[i] = i % 2 === 0 ? 0.5 : -0.5;
  return d;
}

// A 200 Hz tone at 48 kHz: low crossing rate, smooth envelope — speech-like.
function voicedFrame(): Float32Array {
  const d = new Float32Array(512);
  for (let i = 0; i < d.length; i++) d[i] = 0.5 * Math.sin((2 * Math.PI * 200 * i) / 48000);
  return d;
}

function capturing(): LoopState {
  const s = createLoopState(32);
  s.speaking = true;
  return s;
}

describe('utterance endpoint vs. non-speech room noise', () => {
  test('shapeless noise above the gate ends the turn instead of pinning capture open', () => {
    const s = capturing();
    // ~0.5 RMS noise is far above the quiet-room sustain gate, but carries no
    // speech shape — it must accumulate toward the endpoint, not reset it.
    for (let i = 0; i < 40 && s.speaking; i++) handleCaptureFrame(s, noiseFrame(), 0.5, 0.01);
    expect(s.speaking).toBe(false); // old behavior: still open at the 12s ceiling
  });

  test('sustained voiced speech keeps the turn open', () => {
    const s = capturing();
    for (let i = 0; i < 40; i++) handleCaptureFrame(s, voicedFrame(), 0.35, 0.01);
    expect(s.speaking).toBe(true);
  });
});

// The live 2026-08-06 repro: a song playing on the speakers while the caller
// talks. Music is genuinely speech-shaped (it IS singing) and sits above the
// sustain gate, so every frame reset the endpoint and capture ran to the
// ~11.9s MAX_UTTERANCE_FRAMES ceiling — which glued two separate sentences
// into one utterance and sent them to ASR as a single request:
// "Hello, can you pause the song please? Hello, can you pause the song please?"
//
// What tells a media bed from the caller is what barge-in already relies on:
// the caller is LOUD at their own mic, a bed across the room is not.
describe('a media bed must not hold capture open', () => {
  test('a song under the caller lets the utterance end', () => {
    const s = capturing();
    s.userLevel = 0.35; // learned from the caller's own turns
    // Speech-shaped and above the sustain gate, but a fraction of the level
    // the caller actually speaks at.
    for (let i = 0; i < 40 && s.speaking; i++) handleCaptureFrame(s, voicedFrame(), 0.05, 0.01);
    expect(s.speaking).toBe(false);
  });

  test('the caller still holds it open over that same song', () => {
    const s = capturing();
    s.userLevel = 0.35;
    for (let i = 0; i < 40; i++) handleCaptureFrame(s, voicedFrame(), 0.35, 0.01);
    expect(s.speaking).toBe(true);
  });

  // Before the caller has ever been measured there is nothing to compare
  // against — the very first utterance of a call must not be gated away.
  test('an unlearned caller level gates nothing', () => {
    const s = capturing();
    expect(s.userLevel).toBe(0);
    for (let i = 0; i < 40; i++) handleCaptureFrame(s, voicedFrame(), 0.05, 0.01);
    expect(s.speaking).toBe(true);
  });
});
