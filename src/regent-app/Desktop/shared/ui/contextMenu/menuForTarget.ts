// Pure decision of which items the custom right-click menu shows, given what
// was right-clicked. No native Cut/Copy/Paste/Reload/Inspect items ever
// reach the user — only these, and only when they'd actually do something.

export type MenuItemId = 'cut' | 'copy' | 'paste' | 'selectAll';

export interface MenuItem {
  readonly id: MenuItemId;
  readonly enabled: boolean;
}

export interface TargetInfo {
  readonly editable: boolean;
  readonly hasSelection: boolean;
  /** Whether the platform even exposes a clipboard read API here — a locked
   * webview origin can lack it entirely, in which case Paste is omitted
   * rather than shown disabled forever. */
  readonly canPaste: boolean;
}

export function menuForTarget(target: TargetInfo): readonly MenuItem[] {
  const items: MenuItem[] = [];
  if (target.editable) {
    // Editable fields always offer Cut/Copy (disabled without a selection) —
    // the user just right-clicked an input, so the affordance should be there.
    items.push({ id: 'cut', enabled: target.hasSelection });
    items.push({ id: 'copy', enabled: target.hasSelection });
    if (target.canPaste) items.push({ id: 'paste', enabled: true });
  } else if (target.hasSelection) {
    // Read-only text: Copy only makes sense, and only when there's something
    // selected — otherwise it would be a permanently-disabled item for no reason.
    items.push({ id: 'copy', enabled: true });
  }
  items.push({ id: 'selectAll', enabled: true });
  return items;
}
