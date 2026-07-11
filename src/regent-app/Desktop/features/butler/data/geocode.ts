/** A resolved place: its map coordinates plus Nominatim's full display name
 * (the pin's popup shows the name and lat/lon). */
export interface GeoHit {
  readonly lon: number;
  readonly lat: number;
  readonly label: string;
}

// Place → coordinates + label via Nominatim (CSP-allowed). Null on miss/offline.
export async function geocodePlace(query: string): Promise<GeoHit | null> {
  if (query.trim() === '') return null;
  try {
    const res = await fetch(
      `https://nominatim.openstreetmap.org/search?format=json&limit=1&q=${encodeURIComponent(query)}`,
    );
    const hits = (await res.json()) as Array<{ lat?: string; lon?: string; display_name?: string }>;
    const hit = hits[0];
    if (hit?.lat !== undefined && hit.lon !== undefined) {
      return { lon: Number(hit.lon), lat: Number(hit.lat), label: hit.display_name ?? query.trim() };
    }
  } catch {
    // offline / blocked — caller keeps the current view
  }
  return null;
}

/** Extract a place from a given text (a spoken ask OR Regent's reply), or null.
 * Deliberately narrow — only clearly map-shaped asks raise the globe. */
export function placeIntent(heard: string): string | null {
  const patterns = [
    /show (?:me )?(.{2,60}?) on (?:the |a )?map/i,
    /\b(?:where(?:'s| is)|map of|locate|navigate to)\s+(.{2,60}?)[?.!]?\s*$/i,
  ];
  for (const pattern of patterns) {
    const match = heard.match(pattern);
    if (match?.[1] !== undefined) return match[1].trim();
  }
  return null;
}
