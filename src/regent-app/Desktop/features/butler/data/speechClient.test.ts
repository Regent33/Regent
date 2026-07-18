import { describe, expect, test } from 'bun:test';
import { downsamplePcm16 } from './speechClient';

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
