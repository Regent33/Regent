// A relevant image for a topic, via Wikipedia's REST summary API (free, CC).
// Used to hand the user a supplementary picture alongside a diagram — the
// diagram stays the primary backdrop; this just floats beside it. Best-effort:
// no page, no image, or offline all resolve to null and nothing opens.
import type { ContentItem } from '@/features/butler/domain/content';

interface Summary {
  readonly title?: string;
  readonly thumbnail?: { source?: string };
  readonly originalimage?: { source?: string; width?: number };
  readonly type?: string; // "standard" | "disambiguation" | …
}

/** Fetch a topic image as an image ContentItem, or null. Prefers the full
 * image but caps absurdly large originals by falling back to the thumbnail. */
export async function fetchTopicImage(topic: string): Promise<ContentItem | null> {
  const q = topic.trim();
  if (q.length < 2) return null;
  try {
    const res = await fetch(
      `https://en.wikipedia.org/api/rest_v1/page/summary/${encodeURIComponent(q)}?redirect=true`,
      { headers: { accept: 'application/json' } },
    );
    if (!res.ok) return null;
    const data = (await res.json()) as Summary;
    if (data.type === 'disambiguation') return null; // ambiguous — skip rather than mislead
    const big = data.originalimage;
    const url = big?.source && (big.width ?? 0) <= 4000 ? big.source : data.thumbnail?.source;
    if (!url) return null;
    return { id: url, kind: 'image', url, title: data.title ?? q, host: 'wikipedia.org' };
  } catch {
    return null;
  }
}
