import { describe, expect, test } from 'bun:test';
import { resolveMermaidTheme } from './mermaidLoader';

describe('Mermaid effective theme', () => {
  test('explicit caller theme wins', () => {
    expect(resolveMermaidTheme('default', 'dark', true)).toBe('default');
    expect(resolveMermaidTheme('dark', 'light', false)).toBe('dark');
  });

  test('root app theme controls artifact previews and lightboxes', () => {
    expect(resolveMermaidTheme(undefined, 'dark', false)).toBe('dark');
    expect(resolveMermaidTheme(undefined, 'light', true)).toBe('default');
  });

  test('system preference applies when no root theme is stamped', () => {
    expect(resolveMermaidTheme(undefined, null, true)).toBe('dark');
    expect(resolveMermaidTheme(undefined, null, false)).toBe('default');
  });
});
