// Domain test — zero I/O. The bug: a quiet/over-processed mic peaks below the
// fixed 0.015 gate and hangs on 'listening'. The adaptive gate must open for a
// soft mic in a quiet room while leaving the noisy-room ceiling untouched.
import { expect, test } from 'bun:test';
import { VOICE_CEILING, interruptGate, sustainGate, voiceGate } from './vad';

test('quiet room lowers the onset gate so a soft mic still starts a turn', () => {
  const g = voiceGate(0.001);
  expect(g).toBeLessThan(VOICE_CEILING); // more sensitive than the fixed ceiling
  expect(g).toBeGreaterThanOrEqual(0.006); // but not down into hiss territory
  expect(0.008).toBeGreaterThan(g); // a 0.008-peak quiet voice now crosses onset
});

test('noisy room keeps the original ceiling — hard-won tuning untouched', () => {
  expect(voiceGate(0.02)).toBe(VOICE_CEILING);
});

test('sustain sits below onset (hysteresis) yet above the noise floor', () => {
  const floor = 0.001;
  expect(sustainGate(floor)).toBeLessThan(voiceGate(floor)); // easier to stay than to enter
  expect(sustainGate(0.01)).toBeGreaterThan(0.01); // ambient never counts as voice
});

test('barge-in scales to the mic: a quiet mic can still interrupt', () => {
  const quiet = interruptGate(0.001, 0.012); // loudest speech ever seen ~0.012
  expect(0.012).toBeGreaterThan(quiet); // a 0.012 barge-in now cuts Regent off
  const normal = interruptGate(0.001, 0.08); // a loud mic keeps the 0.02 ceiling
  expect(normal).toBe(0.02);
});

test('barge-in never drops below the echo guard (no self-interrupt)', () => {
  // Regent's TTS bleeds in, raising the floor — the gate must rise with it even
  // for a quiet mic, so the reply audio never triggers its own barge-in.
  expect(interruptGate(0.006, 0.012)).toBeGreaterThan(0.006 * 3);
});
