// Pure-logic tests for the graph's camera math and data shaping (Section 7,
// zero I/O). Covers the screen↔world round-trip, zoom-toward-cursor keeping
// the pointed world point fixed, degree counting, and stable kind colours.
import { describe, expect, test } from 'bun:test';
import { centerOn, screenToWorld, worldToScreen, zoomAt } from './camera';
import { computeDegrees, kindColor } from '@/features/graph/viewmodels/useGraphData';

describe('camera', () => {
  test('screen↔world is a round-trip under an offset camera', () => {
    const cam = { x: 120, y: -40, k: 2.5 };
    const w = screenToWorld(cam, 300, 200);
    const back = worldToScreen(cam, w.x, w.y);
    expect(back.x).toBeCloseTo(300, 6);
    expect(back.y).toBeCloseTo(200, 6);
  });

  test('zoomAt pins the world point under the cursor', () => {
    const cam = { x: 50, y: 30, k: 1 };
    const before = screenToWorld(cam, 400, 260);
    const next = zoomAt(cam, 400, 260, 1.4);
    const after = worldToScreen(next, before.x, before.y);
    expect(after.x).toBeCloseTo(400, 6);
    expect(after.y).toBeCloseTo(260, 6);
    expect(next.k).toBeCloseTo(1.4, 6);
  });

  test('zoom is clamped to the allowed range', () => {
    const cam = { x: 0, y: 0, k: 6 };
    expect(zoomAt(cam, 0, 0, 100).k).toBe(8);
    expect(zoomAt({ x: 0, y: 0, k: 0.1 }, 0, 0, 0.001).k).toBe(0.05);
  });

  test('centerOn places the world point at the viewport centre', () => {
    const cam = centerOn(10, 20, 2, 800, 600);
    const s = worldToScreen(cam, 10, 20);
    expect(s.x).toBeCloseTo(400, 6);
    expect(s.y).toBeCloseTo(300, 6);
  });
});

describe('graph data', () => {
  test('computeDegrees counts incident edges on both endpoints', () => {
    const deg = computeDegrees(
      [{ id: 'a' }, { id: 'b' }, { id: 'c' }],
      [
        { src: 'a', dst: 'b', relation: 'r', weight: 1 },
        { src: 'a', dst: 'c', relation: 'r', weight: 1 },
        { src: 'x', dst: 'a', relation: 'r', weight: 1 }, // unknown endpoint ignored
      ],
    );
    expect(deg.get('a')).toBe(3);
    expect(deg.get('b')).toBe(1);
    expect(deg.get('c')).toBe(1);
    expect(deg.has('x')).toBe(false);
  });

  test('kindColor is deterministic and maps known kinds to their fixed hue', () => {
    expect(kindColor('user')).toBe(kindColor('user'));
    expect(kindColor('user')).toBe('#7dcfff');
    // An unknown kind still resolves to a stable palette hex.
    expect(kindColor('quux')).toBe(kindColor('quux'));
    expect(kindColor('quux')).toMatch(/^#[0-9a-f]{6}$/);
  });
});
