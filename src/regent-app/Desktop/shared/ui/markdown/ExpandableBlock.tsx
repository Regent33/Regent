'use client';
// Collapses tall content (long code blocks, tool output) behind a fixed
// height with a fade + chevron toggle — only shown once the content actually
// overflows, measured via ResizeObserver so streaming text that grows past
// the threshold picks up the control automatically.
import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { ChevronDownIcon } from '@/shared/ui/icons';
import { t } from '@/shared/i18n/t';

const COLLAPSED_MAX_PX = 400;

export function ExpandableBlock({ children }: { children: ReactNode }) {
  const s = t().chat.markdown;
  const innerRef = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [overflowing, setOverflowing] = useState(false);

  useLayoutEffect(() => {
    const el = innerRef.current;
    if (!el) return;
    const measure = () => setOverflowing(el.scrollHeight > COLLAPSED_MAX_PX);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  return (
    <div className="relative">
      <div
        ref={innerRef}
        className="overflow-y-auto"
        style={{ maxHeight: expanded ? undefined : `${COLLAPSED_MAX_PX}px` }}
      >
        {children}
      </div>
      {overflowing && (
        <button
          type="button"
          aria-expanded={expanded}
          aria-label={expanded ? s.collapse : s.expand}
          onClick={() => setExpanded((v) => !v)}
          className="absolute inset-x-0 bottom-0 flex h-7 cursor-pointer items-end justify-center bg-gradient-to-t from-hover to-transparent pb-1 text-text-tertiary transition-colors hover:text-text-primary"
        >
          <ChevronDownIcon className={`size-3.5 transition-transform ${expanded ? 'rotate-180' : ''}`} />
        </button>
      )}
    </div>
  );
}
