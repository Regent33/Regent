import { describe, expect, test } from 'bun:test';
import {
  type TerminalChord,
  isFollowClick,
  terminalAction,
} from '@/features/workspace/domain/terminalKeys';

const chord = (over: Partial<TerminalChord>): TerminalChord => ({
  key: 'a',
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  type: 'keydown',
  ...over,
});

describe('terminalAction', () => {
  // The whole reason this function exists. Copying unconditionally would remove
  // the only way to stop a runaway process.
  test('Ctrl+C copies with a selection and interrupts without one', () => {
    const ctrlC = chord({ key: 'c', ctrlKey: true });
    expect(terminalAction(ctrlC, true)).toBe('copy');
    expect(terminalAction(ctrlC, false)).toBe('shell');
  });

  test('Ctrl+Shift+C copies even with nothing selected', () => {
    expect(terminalAction(chord({ key: 'c', ctrlKey: true, shiftKey: true }), false)).toBe('copy');
  });

  test('paste works on both chords', () => {
    expect(terminalAction(chord({ key: 'v', ctrlKey: true }), false)).toBe('paste');
    expect(terminalAction(chord({ key: 'v', ctrlKey: true, shiftKey: true }), false)).toBe('paste');
    expect(terminalAction(chord({ key: 'v', metaKey: true }), false)).toBe('paste');
  });

  test('find opens the search box', () => {
    expect(terminalAction(chord({ key: 'f', ctrlKey: true }), false)).toBe('search');
  });

  // These are the ones it would be unforgivable to steal: readline uses them
  // constantly and a terminal that swallows them is broken.
  test('readline chords are left to the shell', () => {
    for (const key of ['a', 'k', 'e', 'u', 'w', 'r', 'd', 'l', 'p', 'n']) {
      expect(terminalAction(chord({ key, ctrlKey: true }), false)).toBe('shell');
    }
  });

  test('the macOS-only chords need Cmd, not Ctrl', () => {
    expect(terminalAction(chord({ key: 'a', metaKey: true }), false)).toBe('selectAll');
    expect(terminalAction(chord({ key: 'k', metaKey: true }), false)).toBe('clear');
  });

  test('unmodified typing always reaches the shell', () => {
    for (const key of ['a', 'c', 'v', 'f', 'Enter', 'Tab', 'ArrowUp']) {
      expect(terminalAction(chord({ key }), true)).toBe('shell');
    }
  });

  // Acting on keyup as well would fire every action twice.
  test('only keydown is acted on', () => {
    expect(terminalAction(chord({ key: 'c', ctrlKey: true, type: 'keyup' }), true)).toBe('shell');
  });

  test('a new terminal has a chord', () => {
    expect(terminalAction(chord({ key: '5', ctrlKey: true, shiftKey: true }), false)).toBe(
      'newTerminal',
    );
  });
});

describe('isFollowClick', () => {
  test('Ctrl or Cmd follows; a plain click still selects text', () => {
    expect(isFollowClick({ ctrlKey: true, metaKey: false })).toBe(true);
    expect(isFollowClick({ ctrlKey: false, metaKey: true })).toBe(true);
    expect(isFollowClick({ ctrlKey: false, metaKey: false })).toBe(false);
  });
});
