import { describe, expect, test } from 'bun:test';
import { downsamplePcm16, requestCallSession } from './speechClient';

describe('Butler audio resampling', () => {
  test('preserves speech-band DC level from 48 kHz to 16 kHz', () => {
    const input = new Float32Array(480).fill(0.25);
    const pcm = downsamplePcm16(input, 48000);
    expect(pcm.length).toBe(160);
    expect(pcm.every((sample) => Math.abs(sample - 8192) <= 1)).toBe(true);
  });

  test('anti-aliases high-frequency three-sample noise instead of sampling it as speech', () => {
    const input = new Float32Array(480);
    for (let i = 0; i < input.length; i += 3) {
      input[i] = 1;
      input[i + 1] = -1;
      input[i + 2] = 0;
    }
    const pcm = downsamplePcm16(input, 48000);
    expect(Math.max(...pcm.map(Math.abs))).toBeLessThanOrEqual(1);
  });
});

describe('Butler session handshake', () => {
  test('returns success for an accepted call session', async () => {
    const fetcher = (() => Promise.resolve(new Response('{}', { status: 200 }))) as typeof fetch;
    expect(await requestCallSession('token', fetcher, 20_000)).toEqual({
      ok: true,
      value: undefined,
    });
  });

  test('returns a typed server failure', async () => {
    const fetcher = (() =>
      Promise.resolve(new Response('Butler agent is unavailable', { status: 503 }))) as typeof fetch;
    const result = await requestCallSession('token', fetcher, 20_000);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe('voice-session');
  });

  test('returns a typed timeout instead of hanging on Connecting', async () => {
    const fetcher = (() =>
      Promise.reject(new DOMException('The operation timed out', 'TimeoutError'))) as typeof fetch;
    const result = await requestCallSession('token', fetcher, 20_000);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe('voice-session-timeout');
  });
});
