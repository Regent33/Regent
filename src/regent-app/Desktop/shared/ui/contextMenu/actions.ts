// Executes one menu item against the currently focused/selected content.
// execCommand is deprecated but remains the one cross-target (input, textarea,
// contentEditable, plain selected text) way to Cut/Copy/Paste/Select-all
// without hand-rolling per-element-type splicing and React-controlled-input
// event dispatch.
import type { MenuItemId } from '@/shared/ui/contextMenu/menuForTarget';

export function canPasteHere(): boolean {
  return (
    document.queryCommandSupported?.('paste') === true ||
    typeof navigator.clipboard?.readText === 'function'
  );
}

export async function runMenuAction(id: MenuItemId): Promise<void> {
  if (id === 'selectAll') {
    document.execCommand('selectAll');
    return;
  }
  if (id === 'cut' || id === 'copy') {
    document.execCommand(id);
    return;
  }
  // Paste: try the legacy command first (works in most Chromium/WebView2
  // embeds); fall back to the async Clipboard API and insert manually.
  if (document.execCommand('paste')) return;
  const clipboard = navigator.clipboard;
  if (typeof clipboard?.readText !== 'function') return;
  const text = await clipboard.readText().catch(() => undefined);
  if (text === undefined) return;
  const el = document.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? el.value.length;
    const setter = Object.getOwnPropertyDescriptor(
      el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,
      'value',
    )?.set;
    setter?.call(el, el.value.slice(0, start) + text + el.value.slice(end));
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.setSelectionRange(start + text.length, start + text.length);
  } else if (el instanceof HTMLElement && el.isContentEditable) {
    document.execCommand('insertText', false, text);
  }
}
