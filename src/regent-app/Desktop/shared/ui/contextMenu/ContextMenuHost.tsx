'use client';
// Replaces the native WebView2/Chromium right-click menu (Back/Reload/
// Inspect — the single biggest "this is just a browser tab" tell) with a
// small custom one: Cut/Copy/Paste/Select all, only the items that apply to
// whatever was actually clicked. Mounted once at the shell root.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { menuForTarget, type MenuItemId } from '@/shared/ui/contextMenu/menuForTarget';
import { canPasteHere, runMenuAction } from '@/shared/ui/contextMenu/actions';

interface OpenMenu {
  readonly x: number;
  readonly y: number;
  readonly items: readonly { readonly id: MenuItemId; readonly enabled: boolean }[];
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
}

// Clamp so the menu never renders partly off-screen near an edge/corner.
function clamp(x: number, y: number, width: number, height: number): { x: number; y: number } {
  return {
    x: Math.min(x, window.innerWidth - width - 4),
    y: Math.min(y, window.innerHeight - height - 4),
  };
}

const MENU_WIDTH = 160;
const ITEM_HEIGHT = 30;

export function ContextMenuHost() {
  const s = t().contextMenu;
  const [menu, setMenu] = useState<OpenMenu>();

  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      e.preventDefault();
      const items = menuForTarget({
        editable: isEditableTarget(e.target),
        hasSelection: (window.getSelection()?.toString().length ?? 0) > 0,
        canPaste: canPasteHere(),
      });
      if (items.length === 0) return;
      const { x, y } = clamp(e.clientX, e.clientY, MENU_WIDTH, items.length * ITEM_HEIGHT);
      setMenu({ x, y, items });
    };
    const onDismiss = () => setMenu(undefined);
    window.addEventListener('contextmenu', onContextMenu);
    window.addEventListener('mousedown', onDismiss);
    window.addEventListener('scroll', onDismiss, true);
    window.addEventListener('blur', onDismiss);
    return () => {
      window.removeEventListener('contextmenu', onContextMenu);
      window.removeEventListener('mousedown', onDismiss);
      window.removeEventListener('scroll', onDismiss, true);
      window.removeEventListener('blur', onDismiss);
    };
  }, []);

  useEffect(() => {
    if (menu === undefined) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setMenu(undefined);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [menu]);

  if (menu === undefined) return null;

  return (
    <div
      role="menu"
      className="fixed z-[100] w-40 rounded-md border border-stroke-secondary bg-surface py-1 motion-safe:animate-[fadeIn_100ms_ease-out]"
      style={{ left: menu.x, top: menu.y, boxShadow: 'var(--shadow-elev)' }}
      // The host's own window-level `mousedown` dismiss listener fires BEFORE
      // a button's `click` — without stopping it here, clicking an item would
      // unmount the menu (via the outside-click handler) before its own
      // onClick ever ran.
      onMouseDown={(e) => e.stopPropagation()}
    >
      {menu.items.map((item) => (
        <button
          key={item.id}
          type="button"
          role="menuitem"
          disabled={!item.enabled}
          className="block w-full cursor-pointer px-3 py-1.5 text-left text-xs text-text-secondary hover:bg-hover hover:text-text-primary disabled:pointer-events-none disabled:opacity-40"
          onClick={() => {
            setMenu(undefined);
            void runMenuAction(item.id);
          }}
        >
          {s[item.id]}
        </button>
      ))}
    </div>
  );
}
