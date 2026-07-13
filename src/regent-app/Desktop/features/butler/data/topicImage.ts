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

async function summaryOf(title: string): Promise<Summary | null> {
  const res = await fetch(
    `https://en.wikipedia.org/api/rest_v1/page/summary/${encodeURIComponent(title)}?redirect=true`,
    { headers: { accept: 'application/json' } },
  );
  return res.ok ? ((await res.json()) as Summary) : null;
}

// Nearest article title for a free-text query (opensearch). Diagram titles are
// wordy ("How Fuel Injection Works") and almost never match a page name
// exactly — without this hop the summary lookup 404s and no image ever opens.
async function nearestTitle(query: string): Promise<string | null> {
  const res = await fetch(
    `https://en.wikipedia.org/w/api.php?action=opensearch&format=json&origin=*&limit=1&search=${encodeURIComponent(query)}`,
  );
  if (!res.ok) return null;
  const data = (await res.json()) as [string, string[]];
  return data?.[1]?.[0] ?? null;
}

const imageUrl = (s: Summary): string | undefined => {
  const big = s.originalimage;
  return big?.source && (big.width ?? 0) <= 4000 ? big.source : s.thumbnail?.source;
};

/** Fetch a topic image as an image ContentItem, or null. Tries the topic as a
 * page title first, then falls back to the nearest opensearch match. Prefers
 * the full image but caps absurdly large originals via the thumbnail. */
export async function fetchTopicImage(topic: string): Promise<ContentItem | null> {
  const q = topic.trim();
  if (q.length < 2) return null;
  try {
    let data = await summaryOf(q);
    if (!data || data.type === 'disambiguation' || !imageUrl(data)) {
      const found = await nearestTitle(q);
      if (!found || found === q) return null;
      data = await summaryOf(found);
    }
    if (!data || data.type === 'disambiguation') return null; // ambiguous — skip rather than mislead
    const url = imageUrl(data);
    if (!url) return null;
    return { id: url, kind: 'image', url, title: data.title ?? q, host: 'wikipedia.org' };
  } catch {
    return null;
  }
}
