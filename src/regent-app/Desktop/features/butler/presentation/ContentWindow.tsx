'use client';
// Renders a ContentItem by kind inside a (resizable) FloatingWindow: images
// zoom, a recognized YouTube/OpenStreetMap link gets the consent-gated embed,
// document text (when present) renders as markdown, and everything else
// falls back to a plain "open externally" card — reusing the same renderers
// Markdown.tsx already routes chat content through.
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { Markdown } from '@/shared/ui/Markdown';
import { EmbedCard } from '@/shared/ui/markdown/EmbedCard';
import { ZoomableImage } from '@/shared/ui/markdown/ZoomableImage';
import { detectEmbed } from '@/shared/ui/markdown/embedDetect';
import { openExternal } from '@/shared/infrastructure/opener';
import type { ContentItem } from '@/features/butler/domain/content';

function ExternalCard({ item }: { item: ContentItem }) {
  const s = t().chat.markdown;
  return (
    <div className="flex flex-col items-start gap-2">
      <p className="text-sm font-medium text-text-primary">{item.title}</p>
      <p className="text-xs text-text-tertiary">{item.host}</p>
      <Button variant="secondary" size="sm" onClick={() => openExternal(item.url)}>
        {s.embedOpenExternal}
      </Button>
    </div>
  );
}

export function ContentWindow({ item }: { item: ContentItem }) {
  if (item.kind === 'image') return <ZoomableImage src={item.url} alt={item.title} />;

  if (item.kind === 'video') {
    const descriptor = detectEmbed(item.url);
    if (descriptor) return <EmbedCard descriptor={descriptor} />;
    // eslint-disable-next-line jsx-a11y/media-has-caption -- source captions unknown
    return <video controls src={item.url} className="max-w-full rounded-md" />;
  }

  if (item.kind === 'link') {
    const descriptor = detectEmbed(item.url);
    return descriptor ? <EmbedCard descriptor={descriptor} /> : <ExternalCard item={item} />;
  }

  // document
  return item.text !== undefined ? <Markdown text={item.text} /> : <ExternalCard item={item} />;
}
