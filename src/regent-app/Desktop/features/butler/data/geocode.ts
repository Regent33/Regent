/** A resolved place: its map coordinates plus Nominatim's full display name
 * (the marker label shows the name; the coords place it). */
export interface GeoHit {
  readonly lon: number;
  readonly lat: number;
  readonly label: string;
  /** Nominatim's boundingbox as [south, north, west, east] degrees — how BIG
   * the place is. Drives the globe's landing altitude and the street map's
   * fit (a country fits its outline; a landmark sinks to building scale). */
  readonly bbox?: readonly [number, number, number, number];
}

/** A sane [south, north, west, east] box, or null — guards against a stray
 * degenerate Nominatim result (NaN, or south>=north/west>=east) that would
 * hand a downstream map a box it silently renders NOTHING for. */
export function validBbox(
  bbox: GeoHit['bbox'],
): readonly [number, number, number, number] | null {
  if (bbox === undefined) return null;
  const [s, n, w, e] = bbox;
  return Number.isFinite(s) && Number.isFinite(n) && Number.isFinite(w) && Number.isFinite(e) && s < n && w < e
    ? bbox
    : null;
}

// Memoize by normalized query — the same place is looked up by the viewmodel
// (to gate the globe) and again by MapBackdrop (to draw it); one network call
// serves both, and repeats are instant. Only real answers (a hit, or a genuine
// empty result) are cached — a network error isn't, so it can retry.
const cache = new Map<string, GeoHit | null>();

// Nominatim scores every hit 0–1 by real-world significance (population,
// notability). The location CUES below are loose enough to fire on ordinary
// conversation ("where's the bug", "the flight to Denver was late") — and
// nearly any word happens to name SOME obscure hamlet/stream on Earth. A
// floor of 0.2 (small-town-and-up) keeps genuine "where is Manila / the
// Eiffel Tower / Yellowstone" asks working while dropping the trivial-word
// matches that were popping the map during normal chat.
const MIN_IMPORTANCE = 0.2;

// Place → coordinates + label via Nominatim (CSP-allowed). Null on miss/offline.
export async function geocodePlace(query: string): Promise<GeoHit | null> {
  const key = query.trim().toLowerCase();
  if (key === '') return null;
  const cached = cache.get(key);
  if (cached !== undefined) return cached;
  try {
    const res = await fetch(
      `https://nominatim.openstreetmap.org/search?format=json&limit=3&q=${encodeURIComponent(query.trim())}`,
    );
    const hits = (await res.json()) as Array<{
      lat?: string;
      lon?: string;
      display_name?: string;
      boundingbox?: string[];
      importance?: number;
    }>;
    const hit = hits.find((h) => (h.importance ?? 1) >= MIN_IMPORTANCE);
    // Nominatim's boundingbox arrives as ["latmin","latmax","lonmin","lonmax"].
    const bb =
      hit?.boundingbox?.length === 4 ? hit.boundingbox.map(Number) : undefined;
    const bbox =
      bb !== undefined && bb.every(Number.isFinite)
        ? ([bb[0], bb[1], bb[2], bb[3]] as const)
        : undefined;
    const result: GeoHit | null =
      hit?.lat !== undefined && hit.lon !== undefined
        ? {
            lon: Number(hit.lon),
            lat: Number(hit.lat),
            label: hit.display_name ?? query.trim(),
            ...(bbox ? { bbox } : {}),
          }
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
// "where" requires its verb ("where is/are/'s") — bare "where" matched
// conversational filler ("where do we start", "that's where it gets fun").
const CUES =
  'where(?:\'s| is| are|\'re)|map(?:s)? of|locate|navigate to|directions?(?: to)?|take me to|how far (?:is|to)|route to|fly(?:ing)? to|flights? to|driv(?:e|ing) to|capital of|weather (?:in|at|for)';

// A place-shaped span: starts with a letter, runs through words/spaces and the
// light punctuation place names carry (Ā, hyphen, apostrophe, comma, dot).
const SPAN = '([\\p{L}][\\p{L}\\s.\'\\-,]{1,58}?)';
const CUE_RE = new RegExp(`\\b(?:${CUES})\\s+${SPAN}(?=[?.!,;]|\\s+(?:and|on|in|for|to|please|now)\\b|$)`, 'giu');
// "show/pull up/put X on the map" — the map suffix is what makes it a location ask.
const SHOW_MAP_RE = /(?:show(?: me)?|pull up|put|display)\s+(.{2,60}?)\s+on (?:the |a )?maps?\b/giu;
// "where <SUBJECT> is [in/at/near <PLACE>]" — the subject sits BETWEEN 'where'
// and 'is', which the adjacent "where is" cue above misses. This is the natural
// spoken form ("where the Tesla factory is in China"); we take the subject, and
// when a trailing place is named, the more specific "<subject> <place>" too.
const WHERE_SUBJECT_RE =
  /\bwhere\s+([\p{L}][\p{L}\s.'\-]{1,48}?)\s+(?:is|are|'s|'re)\b(?:\s+(?:in|on|at|near)\s+([\p{L}][\p{L}\s.'\-]{1,40}?))?(?=[?.!,;]|\s|$)/giu;

// Once the map ALREADY holds the stage the conversation is demonstrably about
// places, so follow-ups drop the explicit cue: "what about Tokyo", "now
// Japan", "and Rome?". None of these is trustworthy in open conversation —
// which is exactly why they are opt-in, requested only while the map is up
// (see `followUp`). Without them the globe stayed pinned to the FIRST place
// for the rest of the call: the reported "I want to search another place and
// it doesn't move". Every candidate is still geocode-gated afterwards.
const FOLLOW_CUES =
  'what about|how about|what of|and|now|then|next|also|search(?: for)?|look up|find|go to|show me|pull up|zoom (?:to|in on|into)|move to|switch to|jump to';
const FOLLOW_RE = new RegExp(
  `\\b(?:${FOLLOW_CUES})\\s+${SPAN}(?=[?.!,;]|\\s+(?:and|on|in|for|to|please|now)\\b|$)`,
  'giu',
);

// Words a cue can capture but that are never places — question words (a common
// trap: "how"/"what" geocode to obscure towns) and filler.
const STOP = new Set([
  'i', 'the', 'a', 'an', 'no', 'yes', 'ok', 'okay', 'so', 'well', 'hey', 'hi',
  'regent', 'got', 'it', 'sure', 'right', 'now', 'here', 'there', 'this', 'that',
  'them', 'my file', 'my files', 'my stuff',
  'how', 'why', 'what', 'when', 'who', 'which', 'whom', 'whose', 'that one',
]);

// A follow-up cue is loose enough to swallow a whole instruction ("and how
// does it work", "now explain the Krebs cycle"). The geocode gate would drop
// those, but the SYNC `hasPlaceCandidate` has no such backstop and decides
// whether a place ask owns the stage — so a diagram question asked while the
// map is up would wrongly hold the globe. A follow-up span must therefore
// look like a NAME: short, and not opening with a question word, pronoun,
// auxiliary, or request verb.
const FOLLOW_MAX_WORDS = 4;
const NON_PLACE_HEAD = new Set([
  'how', 'why', 'what', 'when', 'who', 'which', 'whose', 'that', 'this', 'it', 'they', 'we',
  'you', 'he', 'she', 'i', 'is', 'are', 'was', 'were', 'do', 'does', 'did', 'can', 'could',
  'should', 'would', 'will', 'about', 'if', 'not', 'much', 'many', 'more', 'again', 'else',
  'explain', 'tell', 'describe', 'compare', 'summarize', 'summarise', 'list', 'draw', 'make',
  'create', 'write', 'give', 'help', 'teach', 'define', 'calculate', 'translate', 'remind',
  'open', 'play', 'send', 'check', 'read', 'build', 'fix', 'add', 'change', 'set', 'stop',
]);

/** A follow-up capture that plausibly names a place rather than a request. */
function looksLikeName(span: string): boolean {
  const words = span.trim().replace(/^(?:the|a|an)\s+/i, '').split(/\s+/);
  const head = words[0]?.toLowerCase();
  return words.length <= FOLLOW_MAX_WORDS && head !== undefined && !NON_PLACE_HEAD.has(head);
}

/** Pull candidate place phrases — ONLY from an explicit location cue ("where is
 * X", "X on the map", "directions to X"…). A bare proper noun in an explanation
 * ("the history of Rome") or an ambiguous verb ("show me how…") is deliberately
 * NOT a candidate, so explaining a topic never hijacks the globe. Each candidate
 * is still geocode-checked before anything opens. */
export function placeCandidates(text: string, followUp = false): string[] {
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
  for (const m of text.matchAll(WHERE_SUBJECT_RE)) {
    // "where IS X is in Y" (a repeated "is" right after "where") lets the
    // non-greedy subject capture swallow that first "is" as its own first
    // word ("is tesla factory" instead of "tesla factory") — strip it back
    // off, the same way a leading article gets stripped above.
    const subject = m[1].replace(/^(?:is|are|was|were)\s+/i, '');
    // Join subject+place with a comma, not a bare space: Nominatim reads
    // "tesla factory, china" as containment (factory IN China) but
    // "tesla factory china" as three loose keywords, which let "Tesla" (the
    // company, HQ'd in the US) outrank the actual Shanghai gigafactory.
    add(m[2] ? `${subject}, ${m[2]}` : subject); // "<subject>, <place>" — the sharper hit
    add(subject); // …and the bare subject as a fallback
  }
  if (followUp) {
    for (const m of text.matchAll(FOLLOW_RE)) if (looksLikeName(m[1])) add(m[1]);
  }
  return [...out];
}

/** True if the text has ANY place-shaped candidate — a cheap sync check so the
 * turn router can avoid flipping the globe off while an async lookup is pending. */
export function hasPlaceCandidate(text: string, followUp = false): boolean {
  return placeCandidates(text, followUp).length > 0;
}

/** Geocode-gate: return the candidate queries that resolve to a real place.
 * `max` caps pins; `maxAttempts` caps network calls so a chatty reply full of
 * proper nouns can't spam Nominatim. Empty ⇒ open nothing. */
export async function resolvePlaces(
  text: string,
  followUp = false,
  max = 3,
  maxAttempts = 6,
): Promise<string[]> {
  const resolved: string[] = [];
  for (const candidate of placeCandidates(text, followUp).slice(0, maxAttempts)) {
    if (resolved.length >= max) break;
    const hit = await geocodePlace(candidate); // cached; sequential respects the rate limit
    if (hit) resolved.push(candidate);
  }
  return resolved;
}
