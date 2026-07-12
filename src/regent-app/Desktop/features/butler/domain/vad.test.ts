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

test('a normal room keeps the tuned ceiling; a loud room tracks ABOVE ambient', () => {
  expect(voiceGate(0.008)).toBe(VOICE_CEILING); // ambient under the ceiling → tuned 0.015 onset
  expect(voiceGate(0.02)).toBeGreaterThan(0.02); // loud room: gate rises above the noise, never below it
});

test('background noise never self-triggers: ambient-level input stays below both gates', () => {
  // At every noise level, an input sitting AT the measured floor must not cross
  // onset (start a spurious turn) nor barge-in (cut Regent off). This is the core
  // guard that keeps a fan / chatter / TTS bleed out of the conversation.
  for (const floor of [0.001, 0.005, 0.01, 0.02, 0.04]) {
    expect(floor).toBeLessThan(voiceGate(floor));
    expect(floor).toBeLessThan(interruptGate(floor));
  }
});

test('sustain sits below onset (hysteresis) yet above the noise floor', () => {
  const floor = 0.001;
  expect(sustainGate(floor)).toBeLessThan(voiceGate(floor)); // easier to stay than to enter
  expect(sustainGate(0.01)).toBeGreaterThan(0.01); // ambient never counts as voice
});

test('barge-in adapts like onset: a quiet mic in a quiet room can still interrupt', () => {
  const quiet = interruptGate(0.001); // soft mic, quiet room
  expect(quiet).toBeLessThan(0.02); // more sensitive than the old fixed 0.02 ceiling
  expect(0.009).toBeGreaterThan(quiet); // a ~0.009 quiet voice now cuts Regent off
  expect(0.006).toBeLessThan(quiet); // ...but a turn-STARTING onset (0.006) still won't — barge-in stays stricter
});

test('barge-in never falls into hiss — voiceGate 0.006 minimum still holds', () => {
  // Even a near-silent room keeps a floor off pure hiss (no self-trigger on nothing).
  expect(interruptGate(0.0002)).toBeGreaterThanOrEqual(0.006 * 1.3 - 1e-9);
});

test('barge-in rises with TTS echo (no self-interrupt)', () => {
  // Regent's TTS bleeds in, raising the noise floor — the gate must rise with it,
  // so the reply audio never triggers its own barge-in.
  expect(interruptGate(0.006)).toBeGreaterThan(0.006 * 3);
  expect(interruptGate(0.02)).toBeGreaterThan(0.015); // a noisy room stays strict
});
