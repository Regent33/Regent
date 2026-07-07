'use client';
// An inline markdown image that opens a full-screen lightbox on click — a
// fixed, scrim-backed overlay closed by clicking the scrim, the image, or
// Esc. Matches the Overlay scrim/fade fidelity without pulling in the full
// Overlay chrome (no close button, no card border — just the image).
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';

export function ZoomableImage({ src, alt }: { src: string; alt?: string }) {
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

  return (
    <>
      <button
        type="button"
        aria-label={s.openImage}
        onClick={() => setOpen(true)}
        className="my-2 block max-w-full cursor-zoom-in rounded-md"
      >
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={src} alt={alt ?? ''} className="max-w-full rounded-md object-contain" />
      </button>
      {open && (
        <div
          role="presentation"
          className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-6 backdrop-blur-[2px] motion-safe:animate-[fadeIn_120ms_ease-out]"
          onClick={() => setOpen(false)}
        >
          <button type="button" aria-label={s.closeImage} className="cursor-zoom-out">
            {/* eslint-disable-next-line @next/next/no-img-element */}
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
