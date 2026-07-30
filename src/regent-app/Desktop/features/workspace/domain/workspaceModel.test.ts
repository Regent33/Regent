import { describe, expect, test } from 'bun:test';
import {
  folderButtonMode,
  isSaveShortcut,
  openFolderMessage,
  languageForPath,
  sessionFolders,
  toGitStatus,
  toTreeEntries,
} from '@/features/workspace/domain/workspaceModel';

describe('isSaveShortcut', () => {
  test('Ctrl+S and Cmd+S both save', () => {
    const chord = { key: 's', ctrlKey: true, metaKey: false };
    expect(isSaveShortcut(chord)).toBe(true);
    expect(isSaveShortcut({ key: 's', ctrlKey: false, metaKey: true })).toBe(true);
  });

  test('capital S (shift held, or caps lock) still saves', () => {
    expect(isSaveShortcut({ key: 'S', ctrlKey: true, metaKey: false })).toBe(true);
  });

  test('a bare s types a character rather than saving', () => {
    expect(isSaveShortcut({ key: 's', ctrlKey: false, metaKey: false })).toBe(false);
  });

  test('other modified keys are not save', () => {
    expect(isSaveShortcut({ key: 'a', ctrlKey: true, metaKey: false })).toBe(false);
  });
});

describe('toTreeEntries', () => {
  test('maps the RPC shape and drops malformed rows', () => {
    const entries = toTreeEntries({
      entries: [
        { name: 'src', path: 'src', kind: 'dir' },
        { name: 'a.ts', path: 'src/a.ts', kind: 'file', bytes: 12 },
        { name: 'no-path' },
        'nonsense',
      ],
    });
    expect(entries).toEqual([
      { name: 'src', path: 'src', isDir: true, bytes: undefined },
      { name: 'a.ts', path: 'src/a.ts', isDir: false, bytes: 12 },
    ]);
  });

  test('a missing or non-array payload is empty, not a throw', () => {
    expect(toTreeEntries(undefined)).toEqual([]);
    expect(toTreeEntries({ entries: null })).toEqual([]);
  });
});

describe('toGitStatus', () => {
  test('reads the fields the toolbar gates on', () => {
    const status = toGitStatus({
      is_repo: true,
      branch: 'main',
      upstream: 'origin/main',
      ahead: 2,
      behind: 0,
      dirty: true,
      entries: [{ path: 'a.ts', status: ' M', staged: false }],
    });
    expect(status.isRepo).toBe(true);
    expect(status.branch).toBe('main');
    expect(status.upstream).toBe('origin/main');
    expect(status.ahead).toBe(2);
    expect(status.dirty).toBe(true);
    expect(status.entries).toHaveLength(1);
  });

  test('a non-repo folder reports isRepo false with safe defaults', () => {
    const status = toGitStatus({ is_repo: false });
    expect(status.isRepo).toBe(false);
    expect(status.dirty).toBe(false);
    expect(status.branch).toBeUndefined();
    expect(status.entries).toEqual([]);
  });

  test('garbage in never throws', () => {
    expect(toGitStatus(undefined).isRepo).toBe(false);
    expect(toGitStatus('nope').isRepo).toBe(false);
  });
});

describe('languageForPath', () => {
  test('maps common extensions for the editor', () => {
    expect(languageForPath('src/main.rs')).toBe('rust');
    expect(languageForPath('a/b.tsx')).toBe('typescript');
    expect(languageForPath('x.json')).toBe('json');
    expect(languageForPath('notes.md')).toBe('markdown');
  });

  test('unknown extensions fall back to plaintext', () => {
    expect(languageForPath('LICENSE')).toBe('plaintext');
    expect(languageForPath('weird.xyz')).toBe('plaintext');
  });
});

describe('sessionFolders', () => {
  const root = String.raw`C:\Users\Ralph\.regent\artifacts`;

  test('picks the top-level folder out of paths written by this session', () => {
    const folders = sessionFolders(
      [
        String.raw`C:\Users\Ralph\.regent\artifacts\butler-mode-site\index.html`,
        String.raw`C:\Users\Ralph\.regent\artifacts\butler-mode-site\app.js`,
        String.raw`C:\Users\Ralph\.regent\artifacts\rizal-intro\deck.pptx`,
      ],
      root,
    );
    expect([...folders].sort()).toEqual(['butler-mode-site', 'rizal-intro']);
  });

  test('accepts forward slashes and is case-insensitive about the root', () => {
    const folders = sessionFolders(['c:/users/ralph/.regent/artifacts/mount-fuji/notes.md'], root);
    expect([...folders]).toEqual(['mount-fuji']);
  });

  test('ignores terminal commands, unrelated paths, and files sitting in the root', () => {
    const folders = sessionFolders(
      [
        '$ npm run build',
        String.raw`D:\some\other\project\main.rs`,
        String.raw`C:\Users\Ralph\.regent\artifacts\loose.txt`,
      ],
      root,
    );
    expect(folders.size).toBe(0);
  });

  test('no details at all yields an empty set', () => {
    expect(sessionFolders([], root).size).toBe(0);
  });
});

describe('folderButtonMode', () => {
  // Reported 2026-07-30: opening the wrong repo was unrecoverable. The picker
  // rendered only while the session was still on the scratch space, so the one
  // control that could fix the mistake disappeared exactly when it was needed,
  // and the only way out was a new conversation.
  test('a session already on a folder still gets a picker', () => {
    expect(folderButtonMode(false)).toBe('change');
  });

  test('the scratch space offers the first open', () => {
    expect(folderButtonMode(true)).toBe('open');
  });

  // The regression in one line: neither state may render nothing.
  test('every state names an affordance', () => {
    for (const isDefault of [true, false]) {
      expect(['open', 'change']).toContain(folderButtonMode(isDefault));
    }
  });
});

describe('openFolderMessage', () => {
  // Reported 2026-07-29: picking a folder did nothing at all. The deacon the
  // app spawns is pinned by REGENT_DEACON_PATH to an older install that has no
  // `workspace.set`, so the call came back -32601 "method not found" — and the
  // handler discarded the failure, leaving the click indistinguishable from a
  // no-op. A failure must always produce something to show.
  test('a success shows nothing', () => {
    expect(openFolderMessage(true)).toBeUndefined();
    expect(openFolderMessage(true, 'ignored')).toBeUndefined();
  });

  test('the real reason is surfaced verbatim', () => {
    expect(openFolderMessage(false, 'method not found: workspace.set')).toBe(
      'method not found: workspace.set',
    );
  });

  test('a failure with no message still surfaces one', () => {
    // The hole that made this invisible: falsy/blank messages must not fall
    // through to "nothing happened".
    for (const blank of [undefined, '', '   ']) {
      const shown = openFolderMessage(false, blank);
      expect(typeof shown).toBe('string');
      expect((shown ?? '').trim().length).toBeGreaterThan(0);
    }
  });
});
