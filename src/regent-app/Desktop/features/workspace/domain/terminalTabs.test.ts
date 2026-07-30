import { describe, expect, test } from 'bun:test';
import {
  NO_TERMINALS,
  activate,
  addTerminal,
  closeTerminal,
  ensureOne,
} from '@/features/workspace/domain/terminalTabs';

const ids = (s: { tabs: readonly { id: number }[] }) => s.tabs.map((t) => t.id);

describe('terminal tabs', () => {
  test('adding focuses the new terminal', () => {
    const one = addTerminal(NO_TERMINALS);
    expect(ids(one)).toEqual([1]);
    expect(one.activeId).toBe(1);
    const two = addTerminal(one);
    expect(ids(two)).toEqual([1, 2]);
    expect(two.activeId).toBe(2);
  });

  test('ids are never reused, so a label cannot reappear on a different shell', () => {
    const two = addTerminal(addTerminal(NO_TERMINALS));
    const afterClose = closeTerminal(two, 2);
    const reopened = addTerminal(afterClose);
    expect(ids(reopened)).toEqual([1, 3]);
  });

  test('closing an inactive tab leaves focus alone', () => {
    const three = addTerminal(addTerminal(addTerminal(NO_TERMINALS)));
    const closed = closeTerminal(three, 1);
    expect(ids(closed)).toEqual([2, 3]);
    expect(closed.activeId).toBe(3);
  });

  // Focus goes to the neighbour, not back to the start — closing tab 2 of 3
  // should not fling the user to tab 1.
  test('closing the active tab focuses the tab to its right', () => {
    const three = addTerminal(addTerminal(addTerminal(NO_TERMINALS)));
    const onTwo = activate(three, 2);
    const closed = closeTerminal(onTwo, 2);
    expect(ids(closed)).toEqual([1, 3]);
    expect(closed.activeId).toBe(3);
  });

  test('closing the last tab falls back to the one on its left', () => {
    const two = addTerminal(addTerminal(NO_TERMINALS));
    const closed = closeTerminal(two, 2);
    expect(closed.activeId).toBe(1);
  });

  test('closing the only tab leaves nothing focused rather than a dangling id', () => {
    const closed = closeTerminal(addTerminal(NO_TERMINALS), 1);
    expect(ids(closed)).toEqual([]);
    expect(closed.activeId).toBeUndefined();
  });

  test('unknown ids are no-ops, not throws', () => {
    const one = addTerminal(NO_TERMINALS);
    expect(closeTerminal(one, 99)).toEqual(one);
    expect(activate(one, 99)).toEqual(one);
  });

  test('ensureOne opens a terminal only when there are none', () => {
    expect(ids(ensureOne(NO_TERMINALS))).toEqual([1]);
    const one = addTerminal(NO_TERMINALS);
    expect(ensureOne(one)).toBe(one);
  });
});
