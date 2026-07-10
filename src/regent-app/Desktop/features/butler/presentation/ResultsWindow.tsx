'use client';
// Auto-popping result cards — the links Regent just spoke about (sites,
// videos, pictures), presented like the JARVIS reference. Click → system
// browser. Cards are REAL reply content, never fabricated.
import { t } from '@/shared/i18n/t';
import { openExternal } from '@/shared/infrastructure/opener';
import type { LinkCard } from '@/features/butler/domain/phase';

export function ResultsWindow({ links }: { links: readonly LinkCard[] }) {
  const s = t().butler.windows;
  if (links.length === 0) return <p className="text-xs text-text-tertiary">{s.resultsEmpty}</p>;

  return (
    <div className="grid grid-cols-2 gap-2">
      {links.map((link) => (
        <button
          key={link.url}
          type="button"
          title={link.url}
          onClick={() => openExternal(link.url)}
          className="cursor-pointer overflow-hidden rounded-md bg-hover text-left transition-opacity duration-100 hover:opacity-80"
        >
          {link.youtubeId !== undefined ? (
            <img
              src={`https://i.ytimg.com/vi/${link.youtubeId}/mqdefault.jpg`}
              alt=""
              className="aspect-video w-full object-cover"
            />
          ) : link.isImage ? (
            <img src={link.url} alt="" className="aspect-video w-full object-cover" />
          ) : null}
          <div className="px-2 py-1.5">
            <p className="truncate text-xs text-text-primary">{link.title}</p>
            <p className="truncate text-[10px] text-text-tertiary">{link.host}</p>
          </div>
        </button>
      ))}
    </div>
  );
}
