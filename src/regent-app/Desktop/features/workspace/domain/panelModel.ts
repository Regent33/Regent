// The bottom panel's pure state: which tabs exist, which one is showing, and
// how tall it is. Kept free of React so it is testable directly — this repo's
// hook tests only ever exercise pure exports (no jsdom is installed).

/** The panel's tabs, in the order VS Code shows them. */
export const PANEL_TABS = ['terminal', 'output', 'debug'] as const;

export type PanelTab = (typeof PANEL_TABS)[number];

/** Anything unrecognised (a stale persisted value, a hand-edited setting)
 * resolves to the terminal rather than rendering an empty panel. */
export function toPanelTab(value: unknown): PanelTab {
  return PANEL_TABS.includes(value as PanelTab) ? (value as PanelTab) : 'terminal';
}

/** Panel height bounds, in px.
 *
 * `MIN` is a tab bar plus a couple of terminal rows — below that the panel is
 * a sliver that costs a drag to make useful again. `MAX_FRACTION` keeps the
 * editor from being squeezed to nothing, which is recoverable but infuriating.
 */
export const PANEL_MIN_HEIGHT = 120;
export const PANEL_DEFAULT_HEIGHT = 260;
const MAX_FRACTION = 0.8;

/** Clamp a dragged height against the available space.
 *
 * Both bounds matter and the order does: on a very short window the fraction
 * cap can fall BELOW `PANEL_MIN_HEIGHT`, and clamping min-last would then hand
 * back a panel taller than the window. The available height wins in that case.
 */
export function clampPanelHeight(height: number, available: number): number {
  const ceiling = Math.max(0, Math.floor(available * MAX_FRACTION));
  if (ceiling <= PANEL_MIN_HEIGHT) return Math.min(Math.max(0, height), ceiling);
  return Math.min(Math.max(height, PANEL_MIN_HEIGHT), ceiling);
}
