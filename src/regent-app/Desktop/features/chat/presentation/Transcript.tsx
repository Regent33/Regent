'use client';
// The scrolling message list — a plain vertical column that auto-scrolls to
// the newest item, skipping the animation for reduced-motion users.
import { useEffect, useRef } from 'react';
import { MessageRow } from '@/features/chat/presentation/MessageRow';
import type { TranscriptItem } from '@/features/chat/domain/transcript';

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
        <MessageRow key={i} item={item} onApproval={onApproval} />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
