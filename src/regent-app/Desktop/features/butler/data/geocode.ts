/** A resolved place: its map coordinates plus Nominatim's full display name
 * (the marker label shows the name; the coords place it). */
export interface GeoHit {
  readonly lon: number;
  readonly lat: number;
  readonly label: string;
}

// Memoize by normalized query — the same place is looked up by the viewmodel
// (to gate the globe) and again by MapBackdrop (to draw it); one network call
// serves both, and repeats are instant. Only real answers (a hit, or a genuine
// empty result) are cached — a network error isn't, so it can retry.
const cache = new Map<string, GeoHit | null>();

// Place → coordinates + label via Nominatim (CSP-allowed). Null on miss/offline.
export async function geocodePlace(query: string): Promise<GeoHit | null> {
  const key = query.trim().toLowerCase();
  if (key === '') return null;
  const cached = cache.get(key);
  if (cached !== undefined) return cached;
  try {
    const res = await fetch(
      `https://nominatim.openstreetmap.org/search?format=json&limit=1&q=${encodeURIComponent(query.trim())}`,
    );
    const hits = (await res.json()) as Array<{ lat?: string; lon?: string; display_name?: string }>;
    const hit = hits[0];
    const result: GeoHit | null =
      hit?.lat !== undefined && hit.lon !== undefined
        ? { lon: Number(hit.lon), lat: Number(hit.lat), label: hit.display_name ?? query.trim() }
        : null;
    cache.set(key, result);
    return result;
  } catch {
    return null; // offline / blocked — don't cache, allow retry
  }
}

// Cue words that clearly signal a LOCATION query (not an explanation). Kept
// tight: ambiguous verbs like "show me" / "find" / "go to" are excluded because
// "show me how X works" / "find my file" / "go to sleep" are not map asks. The
// "show … on the map" case has its own pattern below (it needs the map suffix).
const CUES =
  'where(?:\'s| is| are|\'re)?|map(?:s)? of|locate|navigate to|directions?(?: to)?|take me to|how far (?:is|to)|route to|fly(?:ing)? to|flights? to|driv(?:e|ing) to|capital of|weather (?:in|at|for)';

// A place-shaped span: starts with a letter, runs through words/spaces and the
// light punctuation place names carry (Ā, hyphen, apostrophe, comma, dot).
const SPAN = '([\\p{L}][\\p{L}\\s.\'\\-,]{1,58}?)';
const CUE_RE = new RegExp(`\\b(?:${CUES})\\s+${SPAN}(?=[?.!,;]|\\s+(?:and|on|in|for|to|please|now)\\b|$)`, 'giu');
// "show/pull up/put X on the map" — the map suffix is what makes it a location ask.
const SHOW_MAP_RE = /(?:show(?: me)?|pull up|put|display)\s+(.{2,60}?)\s+on (?:the |a )?maps?\b/giu;

// Words a cue can capture but that are never places — question words (a common
// trap: "how"/"what" geocode to obscure towns) and filler.
const STOP = new Set([
  'i', 'the', 'a', 'an', 'no', 'yes', 'ok', 'okay', 'so', 'well', 'hey', 'hi',
  'regent', 'got', 'it', 'sure', 'right', 'now', 'here', 'there', 'this', 'that',
  'them', 'my file', 'my files', 'my stuff',
  'how', 'why', 'what', 'when', 'who', 'which', 'whom', 'whose', 'that one',
]);

/** Pull candidate place phrases — ONLY from an explicit location cue ("where is
 * X", "X on the map", "directions to X"…). A bare proper noun in an explanation
 * ("the history of Rome") or an ambiguous verb ("show me how…") is deliberately
 * NOT a candidate, so explaining a topic never hijacks the globe. Each candidate
 * is still geocode-checked before anything opens. */
export function placeCandidates(text: string): string[] {
  const out = new Set<string>();
  const add = (raw: string | undefined) => {
    // Strip a leading article — "the Eiffel Tower" geocodes to a US replica,
    // "Eiffel Tower" to Paris; the article measurably changes the ranking.
    const c = raw
      ?.trim()
      .replace(/[?.!,;]+$/, '')
      .replace(/^(?:the|a|an)\s+/i, '')
      .trim();
    if (c && c.length >= 2 && !STOP.has(c.toLowerCase())) out.add(c);
  };
  for (const m of text.matchAll(CUE_RE)) add(m[1]);
  for (const m of text.matchAll(SHOW_MAP_RE)) add(m[1]);
  return [...out];
}

/** True if the text has ANY place-shaped candidate — a cheap sync check so the
 * turn router can avoid flipping the globe off while an async lookup is pending. */
export function hasPlaceCandidate(text: string): boolean {
  return placeCandidates(text).length > 0;
}

/** Geocode-gate: return the candidate queries that resolve to a real place.
 * `max` caps pins; `maxAttempts` caps network calls so a chatty reply full of
 * proper nouns can't spam Nominatim. Empty ⇒ open nothing. */
export async function resolvePlaces(text: string, max = 3, maxAttempts = 6): Promise<string[]> {
  const resolved: string[] = [];
  for (const candidate of placeCandidates(text).slice(0, maxAttempts)) {
    if (resolved.length >= max) break;
    const hit = await geocodePlace(candidate); // cached; sequential respects the rate limit
    if (hit) resolved.push(candidate);
  }
  return resolved;
}
