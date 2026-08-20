import { describe, expect, test } from 'bun:test';
import { classifyImageSrc } from '@/shared/kernel/imageRef';

describe('classifyImageSrc', () => {
  test('data and blob URLs carry their own bytes', () => {
    expect(classifyImageSrc('data:image/png;base64,AAA')).toEqual({
      kind: 'inline',
      path: 'data:image/png;base64,AAA',
    });
    expect(classifyImageSrc('blob:http://tauri.localhost/abc').kind).toBe('inline');
  });

  test('https is remote, http is not (the CSP would block it)', () => {
    expect(classifyImageSrc('https://example.com/cat.png').kind).toBe('remote');
    expect(classifyImageSrc('HTTPS://example.com/cat.png').kind).toBe('remote');
    expect(classifyImageSrc('http://example.com/cat.png').kind).toBe('local');
  });

  test('a Windows drive path is local, not a scheme', () => {
    // `C:` must not read as a URL scheme — schemes are two chars or more.
    expect(classifyImageSrc('C:\\Users\\me\\shot.png')).toEqual({
      kind: 'local',
      path: 'C:\\Users\\me\\shot.png',
    });
    expect(classifyImageSrc('d:/tmp/shot.png').kind).toBe('local');
  });

  test('UNC, POSIX, $REGENT_HOME-relative and bare names are local', () => {
    expect(classifyImageSrc('\\\\host\\share\\shot.png').kind).toBe('local');
    expect(classifyImageSrc('/home/me/.regent/artifacts/a/shot.png').kind).toBe('local');
    expect(classifyImageSrc('$REGENT_HOME/artifacts/a/shot.png').kind).toBe('local');
    expect(classifyImageSrc('shot.png')).toEqual({ kind: 'local', path: 'shot.png' });
  });

  test('file: URLs unwind to the plain path image.get takes', () => {
    expect(classifyImageSrc('file:///C:/Users/me/shot.png')).toEqual({
      kind: 'local',
      path: 'C:/Users/me/shot.png',
    });
    expect(classifyImageSrc('file:///home/me/shot.png').path).toBe('/home/me/shot.png');
    expect(classifyImageSrc('file:/home/me/shot.png').path).toBe('/home/me/shot.png');
    expect(classifyImageSrc('file://host/share/shot.png').path).toBe('host/share/shot.png');
    // Percent-escapes are decoded — a staged name can contain spaces.
    expect(classifyImageSrc('file:///tmp/my%20shot.png').path).toBe('/tmp/my shot.png');
  });

  test('an unloadable scheme falls to local so the deacon refuses it', () => {
    // Never "remote": the page must not be handed `javascript:` or `asset:`.
    expect(classifyImageSrc('javascript:alert(1)').kind).toBe('local');
    expect(classifyImageSrc('asset://localhost/shot.png').kind).toBe('local');
  });

  test('an empty src is local, never a bare render', () => {
    expect(classifyImageSrc('   ')).toEqual({ kind: 'local', path: '' });
  });
});
