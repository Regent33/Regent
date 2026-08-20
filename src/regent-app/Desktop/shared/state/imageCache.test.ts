import { beforeEach, describe, expect, test } from 'bun:test';
import { cacheImage, cachedImage, clearImageCache } from '@/shared/state/imageCache';

describe('imageCache', () => {
  beforeEach(clearImageCache);

  test('round-trips a data URI within one session', () => {
    cacheImage('s1', 'shot.png', 'data:image/png;base64,AAA');
    expect(cachedImage('s1', 'shot.png')).toBe('data:image/png;base64,AAA');
    expect(cachedImage('s1', 'other.png')).toBeUndefined();
  });

  test('switching session drops the previous session bytes', () => {
    cacheImage('s1', 'shot.png', 'data:a');
    expect(cachedImage('s2', 'shot.png')).toBeUndefined();
    // …and going back does not resurrect them.
    expect(cachedImage('s1', 'shot.png')).toBeUndefined();
  });

  test('caps at 20 entries, evicting least recently used', () => {
    for (let i = 0; i < 20; i++) cacheImage('s1', `p${i}.png`, `data:${i}`);
    // Touch the oldest so it is no longer the eviction candidate.
    expect(cachedImage('s1', 'p0.png')).toBe('data:0');
    cacheImage('s1', 'p20.png', 'data:20');

    expect(cachedImage('s1', 'p0.png')).toBe('data:0');
    expect(cachedImage('s1', 'p1.png')).toBeUndefined();
    expect(cachedImage('s1', 'p20.png')).toBe('data:20');
  });
});
