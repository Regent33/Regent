// Static reference map for the shell's keyboard shortcuts — data only, no
// wiring. It exists so KeybindPanel has one place to render from; it does
// NOT centralize the actual key listeners (each stays where its feature
// already owns it: usePalette.ts for Ctrl/⌘K, useOverlayEsc for Esc, Shell.tsx
// for "?"). `newSession`'s Ctrl+N is listed for discoverability only — today
// it is a label-only hint on the rail (see LeftRail's newSessionKbd), not a
// wired global shortcut; centralizing that would touch handlers outside this
// task's scope, so it stays descriptive here rather than claiming it fires.
export interface Keybind {
  readonly action: 'palette' | 'newSession' | 'closeOverlay' | 'keybinds';
  readonly combo: string;
}

export const KEYBINDS: readonly Keybind[] = [
  { action: 'palette', combo: 'Ctrl/⌘ K' },
  { action: 'newSession', combo: 'Ctrl N' },
  { action: 'closeOverlay', combo: 'Esc' },
  { action: 'keybinds', combo: 'Shift ?' },
];
