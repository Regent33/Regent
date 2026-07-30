import { describe, expect, test } from 'bun:test';
import {
  PANEL_MIN_HEIGHT,
  PANEL_TABS,
  clampPanelHeight,
  toPanelTab,
} from '@/features/workspace/domain/panelModel';

describe('toPanelTab', () => {
  test('the three tabs round-trip', () => {
    for (const tab of PANEL_TABS) {
      expect(toPanelTab(tab)).toBe(tab);
    }
  });

  // A stale persisted value or a hand-edited setting must not render an empty
  // panel — the user would see a tab bar over nothing and have no way to tell
  // whether it was broken or just empty.
  test('anything unrecognised falls back to the terminal', () => {
    for (const junk of [undefined, null, '', 'Terminal', 'problems', 7, {}]) {
      expect(toPanelTab(junk)).toBe('terminal');
    }
  });
});

describe('clampPanelHeight', () => {
  test('a normal drag is returned as-is', () => {
    expect(clampPanelHeight(300, 900)).toBe(300);
  });

  test('too short is raised to the minimum', () => {
    expect(clampPanelHeight(10, 900)).toBe(PANEL_MIN_HEIGHT);
  });

  test('too tall leaves the editor some room', () => {
    // 80% of 900 = 720.
    expect(clampPanelHeight(5000, 900)).toBe(720);
  });

  // The ordering trap: on a short window the 80% ceiling drops BELOW the
  // minimum height, and clamping min-last would hand back a panel taller than
  // the window it lives in.
  test('on a tiny window the available space wins over the minimum', () => {
    const clamped = clampPanelHeight(PANEL_MIN_HEIGHT, 100);
    expect(clamped).toBeLessThanOrEqual(80);
    expect(clamped).toBeGreaterThanOrEqual(0);
  });

  test('never returns a negative height', () => {
    expect(clampPanelHeight(-50, 0)).toBe(0);
    expect(clampPanelHeight(-50, 900)).toBe(PANEL_MIN_HEIGHT);
  });
});
