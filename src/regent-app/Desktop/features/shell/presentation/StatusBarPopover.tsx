'use client';
// Shared status-bar popover shell: a trigger button plus a small panel that
// opens upward (it anchors off the bottom status bar), dismissed by outside
// click or Esc. Positioning/dismiss/token styling lifted from
// StatusBarModelMenu — the original of this pattern — so every status-bar
// item's popover behaves identically.
import { useEffect, useRef, type ReactNode } from 'react';

export interface StatusBarPopoverProps {
  readonly open: boolean;
  readonly onToggle: () => void;
  readonly onClose: () => void;
  /** aria-label for both the trigger button and the panel. */
  readonly label: string;
  readonly triggerContent: ReactNode;
  readonly children: ReactNode;
  readonly align?: 'left' | 'right';
  readonly widthClassName?: string;
}

export function StatusBarPopover({
  open,
  onToggle,
  onClose,
  label,
  triggerContent,
  children,
  align = 'left',
  widthClassName = 'w-56',
}: StatusBarPopoverProps) {
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) onClose();
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open, onClose]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-label={label}
        className="cursor-pointer hover:text-text-secondary"
        onClick={onToggle}
      >
        {triggerContent}
      </button>
      {open && (
        <div
          role="dialog"
          aria-label={label}
          className={`absolute bottom-full z-10 mb-1.5 ${align === 'left' ? 'left-0' : 'right-0'} ${widthClassName} rounded-md border border-stroke-secondary bg-surface px-3 py-2 motion-safe:animate-[fadeIn_100ms_ease-out]`}
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          {children}
        </div>
      )}
    </div>
  );
}
