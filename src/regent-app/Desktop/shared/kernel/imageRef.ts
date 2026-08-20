// What kind of image reference a `src` is, so the renderer knows whether the
// webview can load it directly or has to ask the deacon for the bytes.
//
// The Tauri CSP allows `img-src 'self' data: blob: https:` and carries NO
// `asset:` scheme, and the webview has no filesystem access — so a local path
// (`C:\…\shot.png`, `/home/me/.regent/artifacts/…`, a bare `shot.png` chip on
// a staged attachment) can only render once `image.get` has inlined it as a
// data URI. Pure: no fs, no RPC, no window.

export type ImageKind =
  /** Already carries its own bytes — render as-is (`data:`, `blob:`). */
  | "inline"
  /** A remote URL the CSP permits — render as-is. */
  | "remote"
  /** A filesystem path — needs `image.get` before it can be shown. */
  | "local";

export interface ImageRef {
  readonly kind: ImageKind;
  /** The reference with any `file://` wrapper unwound — what `image.get`
   * takes as its `path`. Unchanged for `inline`/`remote`. */
  readonly path: string;
}

/** `scheme:` at the very start, lowercased ("" when there is none). A Windows
 * drive letter (`C:\…`) is deliberately NOT a scheme — schemes are 2+ chars. */
const SCHEME = /^([a-z][a-z0-9+.-]+):/i;

/** `file:///C:/x.png`, `file://host/share/x.png`, `file:/x.png`. */
function fromFileUrl(raw: string): string {
  const rest = raw.slice("file:".length).replace(/^\/\//, "");
  let path = decodeURIComponent(rest);
  // `file:///C:/x` leaves a leading slash in front of the drive letter.
  if (/^\/[a-z]:/i.test(path)) path = path.slice(1);
  return path;
}

/**
 * Classify one image `src`. Anything that isn't a scheme the webview can load
 * itself is treated as a local path — including a bare file name, which is
 * what a transcript attachment chip keeps.
 */
export function classifyImageSrc(src: string): ImageRef {
  const path = src.trim();
  if (path === "") return { kind: "local", path };

  const scheme = SCHEME.exec(path)?.[1].toLowerCase();
  if (scheme === "data" || scheme === "blob") return { kind: "inline", path };
  // `https:` only. `http:` is blocked by the CSP anyway, so calling it remote
  // would render a permanent broken image instead of the error card.
  if (scheme === "https") return { kind: "remote", path };
  if (scheme === "file") return { kind: "local", path: fromFileUrl(path) };
  // Every other scheme (`http:`, `asset:`, `javascript:`, …) is something the
  // page must not load; the deacon then refuses it as an unreadable path.
  if (scheme !== undefined) return { kind: "local", path };

  // No scheme: a UNC share (`\\host\share\x.png`), a drive path (`C:\x.png`),
  // a POSIX path, a `$REGENT_HOME`-relative one, or a bare name.
  return { kind: "local", path };
}

/** Extensions the deacon's `image.get` will inline (its `classify_kind`
 * image bucket) — the test for "should this attachment show as a picture?". */
const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|bmp)$/i;

/** True when a file name reads as an image the app can show inline. */
export const isImageName = (name: string): boolean => IMAGE_EXT.test(name.trim());
