'use client';
// Titlebar user popover: a small panel below the UserIcon button with a
// title, blurb, and a "Manage profiles" item routing to /profiles. Opens
// downward (it anchors off the top titlebar, unlike the status-bar popovers
// which open up); dismissed by outside click or Esc, same as those.
import { useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { UserIcon } from '@/shared/ui/icons';

export function UserMenu() {
  const s = t().shell.titlebar;
  const [open, setOpen] = useState(false);
  const router = useRouter();
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative flex items-stretch">
      <Button
        variant="ghost"
        size="iconTitlebar"
        aria-label={s.account}
        title={s.account}
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <UserIcon />
      </Button>
      {open && (
        <div
          role="dialog"
          aria-label={s.userMenu.title}
          className="absolute right-0 top-full z-10 mt-1.5 w-64 rounded-md border border-stroke-secondary bg-surface p-3 motion-safe:animate-[fadeIn_100ms_ease-out]"
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          <p className="text-sm font-semibold text-text-primary">{s.userMenu.title}</p>
          <p className="mt-1 text-xs text-text-tertiary">{s.userMenu.blurb}</p>
          <button
            type="button"
            className="mt-3 block w-full cursor-pointer rounded-[4px] px-2.5 py-1.5 text-left text-xs text-text-secondary hover:bg-hover hover:text-text-primary"
            onClick={() => {
              setOpen(false);
              router.push('/profiles');
            }}
          >
            {s.userMenu.manage}
          </button>
        </div>
      )}
    </div>
  );
}
