// Pull presentable links out of a reply — markdown links first (they carry
// titles), then bare URLs. YouTube ids and direct images get special cards.
import type { LinkCard } from '@/features/butler/domain/phase';

const MAX_CARDS = 8;

function youtubeId(url: string): string | undefined {
  const m = url.match(/(?:youtube\.com\/watch\?v=|youtu\.be\/)([\w-]{6,20})/);
  return m?.[1];
}

function toCard(url: string, title?: string): LinkCard | null {
  try {
    const parsed = new URL(url);
    if (!/^https?:$/.test(parsed.protocol)) return null;
    return {
      url,
      title: title?.trim() || parsed.hostname.replace(/^www\./, ''),
      host: parsed.hostname.replace(/^www\./, ''),
      youtubeId: youtubeId(url),
      isImage: /\.(png|jpe?g|gif|webp)(\?|$)/i.test(parsed.pathname),
    };
  } catch {
    return null;
  }
}

export function extractLinks(reply: string): LinkCard[] {
  const cards = new Map<string, LinkCard>();
  // [title](url) markdown links…
  for (const m of reply.matchAll(/\[([^\]]{1,80})\]\((https?:\/\/[^\s)]+)\)/g)) {
    const card = toCard(m[2], m[1]);
    if (card) cards.set(card.url, card);
  }
  // …then bare URLs not already captured.
  for (const m of reply.matchAll(/https?:\/\/[^\s)\]}"'<>]+/g)) {
    if (!cards.has(m[0])) {
      const card = toCard(m[0]);
      if (card) cards.set(card.url, card);
    }
  }
  return [...cards.values()].slice(0, MAX_CARDS);
}
