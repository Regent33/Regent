// Data URIs already fetched through `image.get`, so re-rendering a transcript
// (every streamed delta re-renders the whole list) doesn't re-read the same
// file off disk and re-base64 it on every keystroke.
//
// Small on purpose: 20 entries of at most 5 MB each is the ceiling `image.get`
// itself enforces, and a chat rarely shows more pictures than that at once.
// Switching session drops the lot — the next session's images are different
// files, and holding a previous conversation's bytes is pure memory.

/** Insertion order IS the LRU order: a hit re-inserts, a set evicts the head. */
const cache = new Map<string, string>();
const MAX_ENTRIES = 20;

let scope: string | undefined;

/** Drop everything when the session changed since the last call. */
function enter(session: string | undefined): void {
  if (session === scope) return;
  scope = session;
  cache.clear();
}

/** The cached data URI for `path` in this session, or undefined. */
export function cachedImage(session: string | undefined, path: string): string | undefined {
  enter(session);
  const hit = cache.get(path);
  if (hit === undefined) return undefined;
  // Re-insert so the freshly used entry is the newest again.
  cache.delete(path);
  cache.set(path, hit);
  return hit;
}

/** Remember `dataUri` for `path`, evicting the least recently used first. */
export function cacheImage(session: string | undefined, path: string, dataUri: string): void {
  enter(session);
  cache.delete(path);
  cache.set(path, dataUri);
  while (cache.size > MAX_ENTRIES) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
}

/** Test seam / explicit reset — the session switch normally does this. */
export function clearImageCache(): void {
  scope = undefined;
  cache.clear();
}
