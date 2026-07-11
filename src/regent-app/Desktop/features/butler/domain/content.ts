// Content items Butler "hands" the user mid-call — rich media promoted out
// of a reply's links into their own floating window instead of a thumbnail
// card (images, YouTube videos; links/documents are here for ContentWindow
// to render generically, even though today's only producer — classifyLinkCard
// — never emits them). Pure logic only; the registry hook (useContentWindows)
// is the thin dispatcher, same split as domain/presentation.ts.
import type { LinkCard } from '@/features/butler/domain/phase';

export type ContentKind = 'image' | 'video' | 'link' | 'document';

export interface ContentItem {
  /** Dedupe key — the content's URL. Handing the same link twice focuses the
   * existing window instead of opening a second one. */
  readonly id: string;
  readonly kind: ContentKind;
  readonly url: string;
  readonly title: string;
  readonly host: string;
  /** Inline body for `document` items with real text/markdown (not just a
   * link) — absent means ContentWindow falls back to an external-open card. */
  readonly text?: string;
}

export interface ContentWindowState {
  readonly item: ContentItem;
  readonly x: number;
  readonly y: number;
  readonly z: number;
  readonly width: number;
  readonly height: number;
}

const SIZES: Record<ContentKind, { width: number; height: number }> = {
  image: { width: 420, height: 340 },
  video: { width: 480, height: 320 },
  link: { width: 360, height: 260 },
  document: { width: 420, height: 360 },
};

/** Promote a link card to its own content window when it's an image or a
 * YouTube video — the rich cases ResultsWindow's thumbnail grid undersells.
 * Plain links return `null` and stay in the Results list. */
export function classifyLinkCard(card: LinkCard): ContentItem | null {
  if (card.youtubeId !== undefined) {
    return { id: card.url, kind: 'video', url: card.url, title: card.title, host: card.host };
  }
  if (card.isImage) {
    return { id: card.url, kind: 'image', url: card.url, title: card.title, host: card.host };
  }
  return null;
}

/** Split a turn's links into (a) items promoted to their own content window
 * and (b) plain links that still flow to the Results grid — never both. */
export function splitLinks(cards: readonly LinkCard[]): {
  promoted: ContentItem[];
  plain: LinkCard[];
} {
  const promoted: ContentItem[] = [];
  const plain: LinkCard[] = [];
  for (const card of cards) {
    const item = classifyLinkCard(card);
    if (item) promoted.push(item);
    else plain.push(card);
  }
  return { promoted, plain };
}

/** Insert a new content window, staggered where the caller asks — or, if the
 * same content (by id/url) is already open, just raise it to the top instead
 * of stacking a duplicate. Pure: the caller supplies the stagger offset so
 * this never touches `window`/`document`. */
export function openContentWindow(
  windows: readonly ContentWindowState[],
  item: ContentItem,
  stagger: { x: number; y: number },
): readonly ContentWindowState[] {
  const top = windows.reduce((m, w) => Math.max(m, w.z), 0);
  const existing = windows.find((w) => w.item.id === item.id);
  if (existing) {
    return existing.z === top
      ? windows
      : windows.map((w) => (w.item.id === item.id ? { ...w, z: top + 1 } : w));
  }
  const { width, height } = SIZES[item.kind];
  return [...windows, { item, x: stagger.x, y: stagger.y, z: top + 1, width, height }];
}
