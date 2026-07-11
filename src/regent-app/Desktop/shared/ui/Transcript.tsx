'use client';
// The scrolling message list — a plain vertical column that auto-scrolls to
// the newest item, skipping the animation for reduced-motion users. Autoscroll
// is suppressed while the caller reports the user has scrolled away from the
// bottom (`stickToBottom={false}`) — see ChatView's scroll tracking, which
// also renders the floating scroll-to-bottom button.
import { useEffect, useRef } from 'react';
import { Loader } from '@/shared/ui/Loader';
import { MessageRow } from '@/shared/ui/MessageRow';
import type { TranscriptItem } from '@/shared/kernel/transcript';

export function Transcript({
  items,
  busy = false,
  onApproval,
  stickToBottom = true,
  bottomClearance,
}: {
  items: readonly TranscriptItem[];
  /** A turn is in flight — show the pending dots in the reply's slot until
   * the first streamed text arrives (reasoning models think silently first). */
  busy?: boolean;
  onApproval?: (approved: boolean) => void;
  stickToBottom?: boolean;
  /** Height class for the bottom sentinel (e.g. "h-[8.5rem]") — clearance for
   * a composer floating OVER the scroll area. Part of the CONTENT on purpose:
   * bottom padding on the scroll container itself is excluded from the
   * scrollable extent (Chromium), so it never actually cleared anything. The
   * sentinel is what every scroll-to-bottom path targets, so a full scroll
   * always leaves the last message visible above the overlay. */
  bottomClearance?: string;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);
  // This component remounts fresh (ChatView's Loader/Hero/Transcript
  // ternary) the moment items first go from empty to non-empty — history
  // load and a brand-new chat's first send both land here. That first run is
  // a STARTING position, not an animation: jump instantly. Every later
  // append (streaming, the next send) keeps the smooth "follow" scroll.
  const hasScrolledRef = useRef(false);

  useEffect(() => {
    if (!stickToBottom) return;
    const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
    const instant = reduceMotion || !hasScrolledRef.current;
    bottomRef.current?.scrollIntoView({ block: 'end', behavior: instant ? 'auto' : 'smooth' });
    if (items.length > 0) hasScrolledRef.current = true;
  }, [items, stickToBottom]);

  const last = items.at(-1);
  const pending = busy && !(last?.kind === 'assistant' && last.streaming);

  return (
    <div className="mx-auto flex max-w-190 flex-col gap-4 px-6 py-6">
      {items.map((item, i) => (
        <div key={i} className="motion-safe:animate-[fadeIn_150ms_ease-out]">
          <MessageRow item={item} onApproval={onApproval} />
        </div>
      ))}
      {pending && (
        <div className="motion-safe:animate-[fadeIn_150ms_ease-out]">
          <Loader />
        </div>
      )}
      <div ref={bottomRef} className={bottomClearance} />
    </div>
  );
}
