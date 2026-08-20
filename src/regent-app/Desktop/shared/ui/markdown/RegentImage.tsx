'use client';
// THE image in model output and on user attachments. Three states instead of
// one silent broken glyph: a skeleton while the bytes are on their way, the
// picture (with ZoomableImage's lightbox) once they land, and an error card
// carrying the reference itself when they never do.
//
// `data:`/`blob:` and `https:` render straight through — the CSP already
// allows them. A LOCAL path cannot: the CSP has no `asset:` scheme and the
// webview has no filesystem access, so its bytes come from the deacon's
// `image.get` (within-root check + 5 MB cap on that side) and are held in a
// small per-session cache, because every streamed delta re-renders this list.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { classifyImageSrc } from '@/shared/kernel/imageRef';
import { useActiveSession } from '@/shared/state/activeSession';
import { cacheImage, cachedImage } from '@/shared/state/imageCache';
import { ZoomableImage } from '@/shared/ui/markdown/ZoomableImage';

interface ImageGetResult {
  readonly mime?: string;
  readonly data_uri?: string;
}

export function RegentImage({ src, alt }: { src: string; alt?: string }) {
  const s = t().chat.markdown;
  const session = useActiveSession();
  const ref = classifyImageSrc(src);
  const local = ref.kind === 'local';
  // A remote/inline reference is already loadable; a local one starts as
  // whatever the cache has (a re-render must not re-fetch) and is otherwise
  // resolved by the effect below.
  const [resolved, setResolved] = useState<string | undefined>(() =>
    local ? cachedImage(session, ref.path) : ref.path,
  );
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!local) {
      setResolved(ref.path);
      setFailed(false);
      return;
    }
    const hit = cachedImage(session, ref.path);
    if (hit !== undefined) {
      setResolved(hit);
      setFailed(false);
      return;
    }
    let alive = true;
    setResolved(undefined);
    setFailed(false);
    void deaconRequest<ImageGetResult>('image.get', {
      path: ref.path,
      ...(session === undefined ? {} : { session_id: session }),
    }).then((result) => {
      if (!alive) return;
      const uri = result.ok ? result.value?.data_uri : undefined;
      if (typeof uri !== 'string' || uri === '') {
        setFailed(true);
        return;
      }
      cacheImage(session, ref.path, uri);
      setResolved(uri);
    });
    return () => {
      alive = false;
    };
  }, [local, ref.path, session]);

  if (failed) return <ImageError reference={src} label={s.imageFailed} />;
  if (resolved === undefined) {
    return <ZoomableImage src="" alt={alt} status="loading" />;
  }
  return <ZoomableImage src={resolved} alt={alt} onError={() => setFailed(true)} />;
}

/** What is left when there is no picture: say so, and keep the reference on
 * screen as selectable text so the user can copy it somewhere that can open
 * it. Rendered as text — never a link, never markup. */
function ImageError({ reference, label }: { reference: string; label: string }) {
  return (
    <span className="my-2 flex flex-col gap-1 rounded-md bg-hover px-3 py-2">
      <span className="text-xs text-text-secondary">{label}</span>
      <code className="select-all break-all font-mono text-[11px] text-text-tertiary">
        {reference}
      </code>
    </span>
  );
}
