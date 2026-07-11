// Presentation mode — the single "what is centre-stage right now" for Butler
// Mode. Only ONE surface holds the stage at a time (the voice mark, the globe,
// a diagram, or the floating windows); everything else recedes. A pure reducer
// keeps the rule in one place so the viewmodel stays a thin dispatcher.
export type PresentationMode =
  | { kind: 'voice' }
  | { kind: 'map'; places: readonly string[] } // place queries mentioned this turn
  | { kind: 'diagram'; spec: unknown } // payload typed loosely — Phase 4 fills it
  | { kind: 'windows' };

/** Things a turn can surface. `voice` is an explicit dismiss / return. */
export type PresentationEvent =
  | { type: 'places'; places: string[] }
  | { type: 'diagram'; spec: unknown }
  | { type: 'content' }
  | { type: 'voice' };

export const initialPresentation: PresentationMode = { kind: 'voice' };

// Trim, drop blanks, and collapse duplicates while keeping first-seen order —
// heard + reply often name the same place twice.
function cleanPlaces(places: readonly string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of places) {
    const place = raw.trim();
    if (place === '' || seen.has(place.toLowerCase())) continue;
    seen.add(place.toLowerCase());
    out.push(place);
  }
  return out;
}

/**
 * Fold one event into the current mode. Pure. Rules: one mode at a time; a
 * fresh `places` event REPLACES the prior places (same-kind merge); an empty
 * `places` event is a non-trigger and leaves the mode untouched; `voice`
 * dismisses back to the voice mark.
 */
export function nextPresentation(
  current: PresentationMode,
  event: PresentationEvent,
): PresentationMode {
  switch (event.type) {
    case 'places': {
      const places = cleanPlaces(event.places);
      return places.length === 0 ? current : { kind: 'map', places };
    }
    case 'diagram':
      return { kind: 'diagram', spec: event.spec };
    case 'content':
      return { kind: 'windows' };
    case 'voice':
      return current.kind === 'voice' ? current : { kind: 'voice' };
  }
}
