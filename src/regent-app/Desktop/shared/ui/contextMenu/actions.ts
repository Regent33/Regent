// Executes one menu item. Copy/Cut act on text CAPTURED when the menu opened,
// not on the live selection: clicking a menu button moves focus, and
// `execCommand('copy')` then operates on the button (which has no selection)
// and silently copies nothing. That was the "copy gives an empty clipboard"
// bug — so the text is read once, while the selection is still live, and
// written through the same `copyText` helper the rest of the app uses (it
// handles Windows/Tauri, where the app origin isn't a secure context and
// `navigator.clipboard` can be undefined).
import { copyText } from '@/shared/infrastructure/clipboard';
import type { MenuItemId } from '@/shared/ui/contextMenu/menuForTarget';

export function canPasteHere(): boolean {
  return (
    typeof navigator.clipboard?.readText === 'function' ||
    document.queryCommandSupported?.('paste') === true
  );
}

/** Replace the live selection inside an input/textarea/contentEditable. Used
 * by Cut (with '') and Paste (with clipboard text). */
function replaceSelection(text: string): void {
  const el = document.activeElement;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? el.value.length;
    // React tracks the value on the DOM node, so a plain `el.value = …` is
    // reverted on the next render — go through the native setter and fire the
    // event React actually listens for.
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

/** `selected` is the text captured at menu-open time. */
export async function runMenuAction(id: MenuItemId, selected: string): Promise<void> {
  if (id === 'selectAll') {
    const el = document.activeElement;
    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) el.select();
    else document.execCommand('selectAll');
    return;
  }
  if (id === 'copy') {
    if (selected !== '') await copyText(selected);
    return;
  }
  if (id === 'cut') {
    if (selected === '') return;
    await copyText(selected);
    replaceSelection('');
    return;
  }
  // Paste. The async API is preferred; execCommand is the fallback for embeds
  // that expose it (and is a no-op returning false where they don't).
  const read = navigator.clipboard?.readText;
  if (typeof read === 'function') {
    const text = await navigator.clipboard.readText().catch(() => undefined);
    if (text !== undefined && text !== '') replaceSelection(text);
    return;
  }
  document.execCommand('paste');
}
