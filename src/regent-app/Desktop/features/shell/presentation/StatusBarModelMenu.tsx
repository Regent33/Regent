'use client';
// Status-bar model item: click-to-open panel of `model.list` rows; picking
// one calls `model.set` and shows its (backend-authored) note transiently.
// Opens upward — it anchors off the bottom status bar.
import { useEffect, useRef } from 'react';
import { t } from '@/shared/i18n/t';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import type { ModelMenuState } from '@/features/shell/viewmodels/useModelMenu';

export function StatusBarModelMenu({ menu, label }: { menu: ModelMenuState; label: string }) {
  const s = t().shell.status;
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu.open) return;
    const onDocClick = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) menu.close();
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [menu]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-label={s.modelMenuLabel}
        className="cursor-pointer hover:text-text-secondary"
        onClick={menu.toggle}
      >
        {label}
      </button>
      {menu.note !== undefined && <span className="ml-1.5 text-accent">{menu.note}</span>}

      {menu.open && (
        <div
          role="menu"
          aria-label={s.modelMenuLabel}
          className="absolute bottom-full right-0 z-10 mb-1.5 max-h-64 w-64 overflow-y-auto rounded-md border border-stroke-secondary bg-surface py-1 motion-safe:animate-[fadeIn_100ms_ease-out]"
          style={{ boxShadow: 'var(--shadow-elev)' }}
        >
          {menu.loading && (
            <div className="flex justify-center py-2">
              <Loader />
            </div>
          )}
          {menu.error !== undefined && <ErrorState compact description={menu.error} />}
          {!menu.loading &&
            menu.error === undefined &&
            menu.items.map((item) => (
              <button
                key={item.id}
                type="button"
                role="menuitemradio"
                aria-checked={item.current}
                className={`block w-full cursor-pointer truncate px-3 py-1.5 text-left text-xs hover:bg-hover ${
                  item.current ? 'text-text-primary' : 'text-text-secondary hover:text-text-primary'
                }`}
                onClick={() => menu.select(item.id)}
              >
                {item.label}
              </button>
            ))}
        </div>
      )}
    </div>
  );
}
