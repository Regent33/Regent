// Turn-end phase handling: when the loop returns to `listening`, archive the
// exchange, parse any diagram spec out of the raw reply, and route the stage
// (map / diagram / content windows / voice).
import type { CallPhase } from '@/features/butler/domain/phase';
import { nextPresentation } from '@/features/butler/domain/presentation';
import { splitLinks } from '@/features/butler/domain/content';
import { hasPlaceCandidate, resolvePlaces } from '@/features/butler/data/geocode';
import { extractLinks } from '@/features/butler/data/links';
import { extractPresentSpec } from '@/shared/diagram/presentSpec';
import type { SinkDeps } from '@/features/butler/viewmodels/butlerSinks';

export function makeSetPhase(deps: SinkDeps): (phase: CallPhase) => void {
  const {
    isCancelled,
    setState,
    markDiagramReady,
    analyser,
    playAnalyser,
    analyserRef,
    heardRef,
    fullReplyRef,
    prevPhaseRef,
    specShownRef,
    visualExpectedRef,
    mapOpenRef,
  } = deps;
  return (phase) => {
    if (isCancelled()) return;
    if (phase === 'listening') markDiagramReady();
    if (phase === 'thinking') {
      // A server-side noise rejection emits no `heard` line. Clear the
      // prior turn here so returning to listening cannot archive it a
      // second time.
      fullReplyRef.current = '';
      visualExpectedRef.current = false;
    }
    analyserRef.current = phase === 'speaking' ? playAnalyser : analyser;
    const wasListening = prevPhaseRef.current === 'listening';
    prevPhaseRef.current = phase;
    // Turn finished (busy → listening): archive the exchange and route the
    // stage. Parse (and remove) any ```present diagram spec from the RAW
    // reply first; everything downstream works on the cleaned prose.
    if (phase === 'listening' && !wasListening && fullReplyRef.current !== '') {
      const { spec, text } = extractPresentSpec(fullReplyRef.current);
      const found = extractLinks(text);
      const { promoted, plain } = splitLinks(found);
      const heard = heardRef.current;
      // Did the USER ask for a place? (cheap sync check) — only the heard
      // text counts: scanning the assistant's reply summoned the globe
      // whenever an ordinary explanation mentioned "capital of…"/"where
      // is…" in passing. A place ask OWNS the stage (map), so it also wins
      // over any diagram the model volunteered — we hold and let the async
      // geocoder raise the map, rather than flip to voice and flicker.
      const placeAsked = hasPlaceCandidate(heard, mapOpenRef.current);
      setState((s) => {
        // Precedence: place ask → hold for the map; else diagram spec →
        // diagram; else promoted content → windows; else a bare turn
        // yields the stage back to voice; else hold for the async lookup.
        const presentation = placeAsked
          ? s.presentation
          : specShownRef.current
            ? spec
              ? nextPresentation(s.presentation, { type: 'diagram', spec })
              : s.presentation
            : promoted.length > 0
              ? nextPresentation(s.presentation, { type: 'content' })
              : found.length === 0 && s.presentation.kind !== 'voice'
                ? nextPresentation(s.presentation, { type: 'voice' })
                : s.presentation;
        return {
          ...s,
          phase,
          reply: '',
          log: [...s.log, { who: 'regent', text }],
          links: plain.length > 0 ? plain : s.links,
          content: promoted.length > 0 ? promoted : s.content,
          presentation,
        };
      });
      // Geocode-gate the whole turn: any candidate FROM THE USER'S ASK
      // that resolves to a real place raises the globe with those pins;
      // none resolving leaves a stale globe only if the turn truly moved
      // on (no links). The reply is deliberately not scanned — the map
      // opens because the user asked, never because the answer mentioned
      // a country.
      if (placeAsked) {
        void (async () => {
          const places = await resolvePlaces(heard, mapOpenRef.current);
          if (isCancelled()) return;
          if (places.length > 0) {
            setState((s) => ({ ...s, presentation: nextPresentation(s.presentation, { type: 'places', places }) }));
          } else if (found.length === 0) {
            setState((s) =>
              s.presentation.kind === 'map'
                ? { ...s, presentation: nextPresentation(s.presentation, { type: 'voice' }) }
                : s,
            );
          }
        })();
      }
      return;
    }
    setState((s) => ({ ...s, phase }));
  };
}
