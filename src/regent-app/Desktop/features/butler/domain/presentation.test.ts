// Pure-logic tests for the presentation-mode reducer (zero I/O). Covers the
// one-mode-at-a-time rule, same-kind place replacement, non-triggering empty
// events, and dismiss-to-voice.
import { describe, expect, test } from 'bun:test';
import { initialPresentation, nextPresentation } from './presentation';

describe('presentation', () => {
  test('starts on the voice mark', () => {
    expect(initialPresentation).toEqual({ kind: 'voice' });
  });

  test('a places event takes the stage as the map', () => {
    const next = nextPresentation(initialPresentation, { type: 'places', places: ['Paris'] });
    expect(next).toEqual({ kind: 'map', places: ['Paris'] });
  });

  test('a fresh places event replaces the prior places (same-kind merge)', () => {
    const first = nextPresentation(initialPresentation, { type: 'places', places: ['Paris'] });
    const second = nextPresentation(first, { type: 'places', places: ['Rome', 'Milan'] });
    expect(second).toEqual({ kind: 'map', places: ['Rome', 'Milan'] });
  });

  test('places are trimmed and de-duplicated case-insensitively', () => {
    const next = nextPresentation(initialPresentation, {
      type: 'places',
      places: ['  Paris ', 'paris', 'Rome', ''],
    });
    expect(next).toEqual({ kind: 'map', places: ['Paris', 'Rome'] });
  });

  test('an empty places event is a non-trigger and keeps the current mode', () => {
    const map = nextPresentation(initialPresentation, { type: 'places', places: ['Paris'] });
    expect(nextPresentation(map, { type: 'places', places: [] })).toBe(map);
    expect(nextPresentation(map, { type: 'places', places: ['   '] })).toBe(map);
  });

  test('voice dismisses back from any mode, and is idempotent from voice', () => {
    const map = nextPresentation(initialPresentation, { type: 'places', places: ['Paris'] });
    expect(nextPresentation(map, { type: 'voice' })).toEqual({ kind: 'voice' });
    // Already on voice → same reference (no needless re-render).
    expect(nextPresentation(initialPresentation, { type: 'voice' })).toBe(initialPresentation);
  });

  test('diagram and content events swap the stage', () => {
    expect(nextPresentation(initialPresentation, { type: 'diagram', spec: { n: 1 } })).toEqual({
      kind: 'diagram',
      spec: { n: 1 },
    });
    expect(nextPresentation(initialPresentation, { type: 'content' })).toEqual({ kind: 'windows' });
  });
});
