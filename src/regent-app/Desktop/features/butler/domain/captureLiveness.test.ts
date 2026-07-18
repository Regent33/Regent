import { expect, test } from 'bun:test';
import { CAPTURE_STALE_MS, captureNeedsRestart } from './captureLiveness';

test('a live capture heartbeat stays connected', () => {
  expect(captureNeedsRestart(9_000, 10_000, false, true)).toBe(false);
});

test('a dead track or stale audio callback restarts visible Butler capture', () => {
  expect(captureNeedsRestart(10_000 - CAPTURE_STALE_MS, 10_000, false, true)).toBe(true);
  expect(captureNeedsRestart(9_999, 10_000, true, true)).toBe(true);
});

test('background tabs do not churn microphone sessions', () => {
  expect(captureNeedsRestart(0, CAPTURE_STALE_MS * 2, true, false)).toBe(false);
});
