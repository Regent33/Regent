// Which terminals exist and which one is showing. Pure — no React, no RPC — so
// the awkward cases (closing the active tab, closing the last one) are tested
// rather than discovered.

export interface TerminalTabState {
  readonly id: number;
  /** Shown on the tab. Stable for the tab's life: renumbering on close would
   * move labels under the pointer mid-click. */
  readonly label: string;
}

export interface TerminalsState {
  readonly tabs: readonly TerminalTabState[];
  readonly activeId: number | undefined;
  /** Monotonic — never reused, so a closed tab's label never reappears while
   * its shell is still shutting down. */
  readonly nextId: number;
}

export const NO_TERMINALS: TerminalsState = { tabs: [], activeId: undefined, nextId: 1 };

/** Adds a terminal and focuses it — opening one you then have to click is a
 * pointless second step. */
export function addTerminal(state: TerminalsState): TerminalsState {
  const id = state.nextId;
  return {
    tabs: [...state.tabs, { id, label: String(id) }],
    activeId: id,
    nextId: id + 1,
  };
}

/** Closes `id`.
 *
 * Focus moves to the NEIGHBOUR — the tab to the right, else the one to the left
 * — which is where the eye already is. Falling back to "first tab" would jump
 * the user across the strip after closing tab 5 of 6.
 */
export function closeTerminal(state: TerminalsState, id: number): TerminalsState {
  const index = state.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return state;
  const tabs = state.tabs.filter((tab) => tab.id !== id);
  if (state.activeId !== id) return { ...state, tabs };
  const neighbour = tabs[index] ?? tabs[index - 1];
  return { ...state, tabs, activeId: neighbour?.id };
}

export function activate(state: TerminalsState, id: number): TerminalsState {
  return state.tabs.some((tab) => tab.id === id) ? { ...state, activeId: id } : state;
}

/** Ensures at least one terminal exists — the panel opening on an empty strip
 * would make the user click "+" before they could type. */
export function ensureOne(state: TerminalsState): TerminalsState {
  return state.tabs.length === 0 ? addTerminal(state) : state;
}
