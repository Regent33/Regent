'use client';
// An inline markdown image that opens a full-screen lightbox on click — a
// fixed, scrim-backed overlay closed by clicking the scrim, the image, or
// Esc. Matches the Overlay scrim/fade fidelity without pulling in the full
// Overlay chrome (no close button, no card border — just the image).
//
// `status` is the caller's fetch state, not the browser's: RegentImage passes
// 'loading' while a local path is still travelling through `image.get`, and
// the picture slot holds a skeleton until then. Nothing about the lightbox
// changes with it — an image that isn't there yet simply isn't clickable.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';

export function ZoomableImage({
  src,
  alt,
  status = 'ready',
  onError,
}: {
  src: string;
  alt?: string;
  status?: 'loading' | 'ready';
  onError?: () => void;
}) {
  const s = t().chat.markdown;
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open]);

  if (status === 'loading') {
    // A span, not a div: markdown puts images inside a paragraph.
    return (
      <span
        role="status"
        aria-label={s.imageLoading}
        className="my-2 block h-40 w-full max-w-[320px] animate-pulse rounded-md bg-hover"
      />
    );
  }

  return (
    <>
      <button
        type="button"
        aria-label={s.openImage}
        onClick={() => setOpen(true)}
        className="my-2 block max-w-full cursor-zoom-in rounded-md"
      >
        <img
          src={src}
          alt={alt ?? ''}
          loading="lazy"
          onError={onError}
          className="max-w-full rounded-md object-contain"
        />
      </button>
      {open && (
        <div
          role="presentation"
          className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-6 backdrop-blur-[2px] motion-safe:animate-[fadeIn_120ms_ease-out]"
          onClick={() => setOpen(false)}
        >
          <button type="button" aria-label={s.closeImage} className="cursor-zoom-out">
            <img
              src={src}
              alt={alt ?? ''}
              className="max-h-[90vh] max-w-[90vw] rounded-md object-contain"
              style={{ boxShadow: 'var(--shadow-elev)' }}
            />
          </button>
        </div>
      )}
    </>
  );
}
