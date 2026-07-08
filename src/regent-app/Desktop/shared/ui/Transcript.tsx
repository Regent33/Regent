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
}: {
  items: readonly TranscriptItem[];
  /** A turn is in flight — show the pending dots in the reply's slot until
   * the first streamed text arrives (reasoning models think silently first). */
  busy?: boolean;
  onApproval?: (approved: boolean) => void;
  stickToBottom?: boolean;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!stickToBottom) return;
    const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
    bottomRef.current?.scrollIntoView({ block: 'end', behavior: reduceMotion ? 'auto' : 'smooth' });
  }, [items, stickToBottom]);

  const last = items.at(-1);
  const pending = busy && !(last?.kind === 'assistant' && last.streaming);

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-4 px-6 py-6">
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
      <div ref={bottomRef} />
    </div>
  );
}
