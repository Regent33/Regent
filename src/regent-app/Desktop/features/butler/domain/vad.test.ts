// Domain test — zero I/O. The bug: a quiet/over-processed mic peaks below the
// fixed 0.015 gate and hangs on 'listening'. The adaptive gate must open for a
// soft mic in a quiet room while leaving the noisy-room ceiling untouched.
import { expect, test } from 'bun:test';
import { VOICE_CEILING, sustainGate, voiceGate } from './vad';

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
