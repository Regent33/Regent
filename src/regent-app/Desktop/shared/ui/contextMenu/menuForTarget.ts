// Pure decision of which items the custom right-click menu shows, given what
// was right-clicked. No native Cut/Copy/Paste/Reload/Inspect items ever
// reach the user — only these, and only enabled when they'd actually do
// something.
//
// All four items are ALWAYS present. An earlier version omitted the ones that
// didn't apply, so right-clicking chat text produced a menu containing just
// "Select all" — which reads as broken rather than minimal. A greyed-out Copy
// tells the user "select something first"; a missing Copy tells them nothing.

export type MenuItemId = 'cut' | 'copy' | 'paste' | 'selectAll';

export interface MenuItem {
  readonly id: MenuItemId;
  readonly enabled: boolean;
}

export interface TargetInfo {
  readonly editable: boolean;
  readonly hasSelection: boolean;
  /** Whether this webview can read the clipboard at all. On Windows the Tauri
   * origin isn't a secure context, so `navigator.clipboard` can be missing —
   * Paste is then shown disabled rather than silently absent. */
  readonly canPaste: boolean;
}

export function menuForTarget(target: TargetInfo): readonly MenuItem[] {
  return [
    // Cut removes text, so it needs both a selection and somewhere to remove
    // it FROM — selected chat output can be copied but not cut.
    { id: 'cut', enabled: target.editable && target.hasSelection },
    { id: 'copy', enabled: target.hasSelection },
    { id: 'paste', enabled: target.editable && target.canPaste },
    { id: 'selectAll', enabled: true },
  ];
}
