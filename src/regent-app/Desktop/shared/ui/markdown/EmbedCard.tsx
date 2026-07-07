'use client';
// A consent-gated embed: label + (YouTube) thumbnail + a "Load" button — the
// iframe never mounts until the user clicks it, so no third-party frame or
// tracker loads just from rendering a chat message. Always keeps an "open
// externally" fallback link (via the shell opener) alongside the gate.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { openExternal } from '@/shared/infrastructure/opener';
import type { EmbedDescriptor } from '@/shared/ui/markdown/embedDetect';

function PlayIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" className="size-5" aria-hidden>
      <path d="M8 5.5v13l11-6.5-11-6.5Z" />
    </svg>
  );
}

function PinIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-6"
      aria-hidden
    >
      <path d="M12 21s-7-6.2-7-11a7 7 0 0 1 14 0c0 4.8-7 11-7 11Z" />
      <circle cx="12" cy="10" r="2.5" />
    </svg>
  );
}

function ExternalIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-3.5"
      aria-hidden
    >
      <path d="M9 15 20 4M20 4h-6M20 4v6" />
      <path d="M20 13v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h6" />
    </svg>
  );
}

const YOUTUBE_ALLOW =
  'accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share';

function Iframe({ descriptor }: { descriptor: EmbedDescriptor }) {
  if (descriptor.provider === 'youtube') {
    return (
      <iframe
        src={descriptor.embedUrl}
        title={descriptor.label}
        loading="lazy"
        referrerPolicy="no-referrer"
        allow={YOUTUBE_ALLOW}
        allowFullScreen
        className="block aspect-video w-full border-0"
      />
    );
  }
  return (
    <iframe
      src={descriptor.embedUrl}
      title={descriptor.label}
      loading="lazy"
      referrerPolicy="no-referrer"
      className="block h-[300px] w-full border-0"
    />
  );
}

export function EmbedCard({ descriptor }: { descriptor: EmbedDescriptor }) {
  const s = t().chat.markdown;
  const [loaded, setLoaded] = useState(false);

  return (
    <div className="my-2 max-w-[420px] overflow-hidden rounded-md bg-hover">
      <div className="flex items-center justify-between gap-2 px-3 py-1.5">
        <span className="font-mono text-[11px] uppercase tracking-[0.04em] text-text-tertiary">
          {descriptor.label}
        </span>
        <button
          type="button"
          onClick={() => openExternal(descriptor.sourceUrl)}
          className="flex shrink-0 items-center gap-1 rounded-[4px] p-1 text-text-tertiary transition-colors hover:bg-stroke-secondary hover:text-text-primary"
          aria-label={s.embedOpenExternal}
          title={s.embedOpenExternal}
        >
          <ExternalIcon />
        </button>
      </div>
      {loaded ? (
        <Iframe descriptor={descriptor} />
      ) : (
        <button
          type="button"
          onClick={() => setLoaded(true)}
          aria-label={`${s.embedLoad} ${descriptor.label}`}
          className="group relative flex w-full items-center justify-center overflow-hidden bg-stroke-tertiary"
          style={{ aspectRatio: descriptor.provider === 'youtube' ? 16 / 9 : 16 / 10 }}
        >
          {descriptor.provider === 'youtube' && (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src={descriptor.thumbnailUrl}
              alt=""
              loading="lazy"
              className="absolute inset-0 size-full object-cover opacity-90 transition-opacity group-hover:opacity-100"
            />
          )}
          <span className="relative flex items-center gap-2 rounded-full bg-scrim px-4 py-2 text-sm font-medium text-on-accent">
            {descriptor.provider === 'youtube' ? <PlayIcon /> : <PinIcon />}
            {s.embedLoad}
          </span>
        </button>
      )}
    </div>
  );
}
