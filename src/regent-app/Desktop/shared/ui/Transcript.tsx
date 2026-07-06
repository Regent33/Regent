'use client';
// The scrolling message list — a plain vertical column that auto-scrolls to
// the newest item, skipping the animation for reduced-motion users.
import { useEffect, useRef } from 'react';
import { MessageRow } from '@/shared/ui/MessageRow';
import type { TranscriptItem } from '@/shared/kernel/transcript';

export function Transcript({
  items,
  onApproval,
}: {
  items: readonly TranscriptItem[];
  onApproval?: (approved: boolean) => void;
}) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
    bottomRef.current?.scrollIntoView({ block: 'end', behavior: reduceMotion ? 'auto' : 'smooth' });
  }, [items]);

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-4 px-6 py-6">
      {items.map((item, i) => (
        <div key={i} className="motion-safe:animate-[fadeIn_150ms_ease-out]">
          <MessageRow item={item} onApproval={onApproval} />
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
