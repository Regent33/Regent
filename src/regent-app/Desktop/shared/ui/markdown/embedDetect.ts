// Pure URL matchers for the two privacy-safe embed providers this app
// supports. No custom remark plugin is needed — remark-gfm already turns
// bare URLs and `[text](url)` links into anchors, so Markdown.tsx's `a`
// override just runs every href through `detectEmbed` and swaps in an
// EmbedCard when it recognizes one; anything else stays a plain link.
export type EmbedDescriptor =
  | { provider: 'youtube'; id: string; embedUrl: string; thumbnailUrl: string; sourceUrl: string; label: string }
  | { provider: 'openstreetmap'; embedUrl: string; sourceUrl: string; label: string };

const YOUTUBE_ID_RE = /^[A-Za-z0-9_-]{11}$/;

function bareHost(hostname: string): string {
  return hostname.replace(/^www\./, '').replace(/^m\./, '');
}

function detectYouTube(url: URL): EmbedDescriptor | undefined {
  const host = bareHost(url.hostname);
  const segments = url.pathname.split('/').filter(Boolean);
  let id = '';
  if (host === 'youtu.be') {
    id = segments[0] ?? '';
  } else if (host === 'youtube.com' || host === 'youtube-nocookie.com') {
    if (segments[0] === 'watch') id = url.searchParams.get('v') ?? '';
    else if (segments[0] === 'shorts' || segments[0] === 'embed' || segments[0] === 'live') id = segments[1] ?? '';
  } else {
    return undefined;
  }
  if (!YOUTUBE_ID_RE.test(id)) return undefined;
  return {
    provider: 'youtube',
    id,
    // Privacy-enhanced host only — never youtube.com/embed.
    embedUrl: `https://www.youtube-nocookie.com/embed/${id}`,
    thumbnailUrl: `https://i.ytimg.com/vi/${id}/hqdefault.jpg`,
    sourceUrl: url.toString(),
    label: 'YouTube',
  };
}

// OSM's share links carry state in the fragment (`#map=zoom/lat/lng`), which
// `URL` still parses on an `https://www.openstreetmap.org/...` link. Without
// that anchor there's no bbox to build, so the link falls back to a plain
// anchor instead of a broken embed.
function detectOpenStreetMap(url: URL): EmbedDescriptor | undefined {
  if (bareHost(url.hostname) !== 'openstreetmap.org') return undefined;
  const match = /map=(\d+(?:\.\d+)?)\/(-?\d+(?:\.\d+)?)\/(-?\d+(?:\.\d+)?)/.exec(url.hash);
  if (!match) return undefined;
  const [, zoomStr, latStr, lngStr] = match;
  const zoom = Number(zoomStr);
  const lat = Number(latStr);
  const lng = Number(lngStr);
  const lonDelta = 360 / 2 ** zoom;
  const latDelta = lonDelta / 2;
  const bbox = [lng - lonDelta / 2, lat - latDelta / 2, lng + lonDelta / 2, lat + latDelta / 2]
    .map((v) => v.toFixed(5))
    .join(',');
  const params = new URLSearchParams({ bbox, layer: 'mapnik', marker: `${lat},${lng}` });
  return {
    provider: 'openstreetmap',
    embedUrl: `https://www.openstreetmap.org/export/embed.html?${params.toString()}`,
    sourceUrl: url.toString(),
    label: 'OpenStreetMap',
  };
}

/** Best-effort embed match for a markdown link href. Returns `undefined` for
 * anything that isn't a recognized YouTube video or an OpenStreetMap link
 * with an embeddable coordinate anchor — the caller renders a plain external
 * link in that case. */
export function detectEmbed(href: string): EmbedDescriptor | undefined {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return undefined;
  }
  if (url.protocol !== 'https:' && url.protocol !== 'http:') return undefined;
  return detectYouTube(url) ?? detectOpenStreetMap(url);
}
