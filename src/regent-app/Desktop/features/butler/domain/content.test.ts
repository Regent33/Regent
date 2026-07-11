// Pure-logic tests for content-window classification and the dedupe-by-id
// insert rule (zero I/O). Style of ../domain/presentation.test.ts.
import { describe, expect, test } from 'bun:test';
import { classifyLinkCard, openContentWindow, splitLinks } from './content';
import type { LinkCard } from './phase';

const image: LinkCard = { url: 'https://x.test/pic.png', title: 'Pic', host: 'x.test', isImage: true };
const video: LinkCard = {
  url: 'https://youtu.be/abc123ABCDE',
  title: 'Vid',
  host: 'youtu.be',
  youtubeId: 'abc123ABCDE',
  isImage: false,
};
const plain: LinkCard = { url: 'https://x.test/page', title: 'Page', host: 'x.test', isImage: false };

describe('classifyLinkCard', () => {
  test('promotes images and YouTube links; plain links stay null', () => {
    expect(classifyLinkCard(image)).toEqual({
      id: image.url,
      kind: 'image',
      url: image.url,
      title: 'Pic',
      host: 'x.test',
    });
    expect(classifyLinkCard(video)).toEqual({
      id: video.url,
      kind: 'video',
      url: video.url,
      title: 'Vid',
      host: 'youtu.be',
    });
    expect(classifyLinkCard(plain)).toBeNull();
  });
});

describe('splitLinks', () => {
  test('separates promoted items from the plain links Results keeps', () => {
    const { promoted, plain: kept } = splitLinks([image, video, plain]);
    expect(promoted.map((p) => p.kind)).toEqual(['image', 'video']);
    expect(kept).toEqual([plain]);
  });

  test('a links list with nothing promotable keeps everything in `plain`', () => {
    const { promoted, plain: kept } = splitLinks([plain]);
    expect(promoted).toEqual([]);
    expect(kept).toEqual([plain]);
  });
});

describe('openContentWindow', () => {
  test('adds a new window on top, staggered where asked', () => {
    const windows = openContentWindow([], classifyLinkCard(image)!, { x: 48, y: 96 });
    expect(windows).toHaveLength(1);
    expect(windows[0]).toMatchObject({ x: 48, y: 96, z: 1 });
  });

  test('handing the same content twice focuses the existing window instead of duplicating', () => {
    const item = classifyLinkCard(image)!;
    const w1 = openContentWindow([], item, { x: 48, y: 96 });
    const w2 = openContentWindow(w1, classifyLinkCard(video)!, { x: 72, y: 128 });
    const w3 = openContentWindow(w2, item, { x: 999, y: 999 }); // stagger ignored — dedupe by id
    expect(w3).toHaveLength(2);
    expect(w3.find((w) => w.item.id === item.id)).toMatchObject({ x: 48, y: 96, z: 3 });
  });

  test('is a no-op (same array reference) when the window is already on top', () => {
    const item = classifyLinkCard(image)!;
    const w1 = openContentWindow([], item, { x: 48, y: 96 });
    const w2 = openContentWindow(w1, item, { x: 999, y: 999 });
    expect(w2).toBe(w1);
  });
});
