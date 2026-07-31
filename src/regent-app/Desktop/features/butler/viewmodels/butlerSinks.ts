// The CallSinks the VAD loop drives: caption/reply/error updates, the
// visual-first gate, and turn-end diagram recovery. The phase handler (turn
// archiving + stage routing) lives in butlerPhase.ts.
import type { Dispatch, SetStateAction } from 'react';
import type { ButlerState } from '@/features/butler/domain/phase';
import {
  createVisualReadyGate,
  expectsVisualExplanation,
  fallbackPresentSpec,
  isConversationalTurn,
  type VisualReadyGate,
} from '@/features/butler/domain/visualReady';
import { nextPresentation } from '@/features/butler/domain/presentation';
import { isExitButlerCommand } from '@/features/butler/domain/exitIntent';
import type { CallSinks } from '@/features/butler/data/callTypes';
import { recoverDiagramArtifact } from '@/features/butler/data/diagramArtifact';
import { hasPlaceCandidate, resolvePlaces } from '@/features/butler/data/geocode';
import { fetchTopicImage } from '@/features/butler/data/topicImage';
import { extractPresentSpec, stripPresentTail } from '@/shared/diagram/presentSpec';
import { makeSetPhase } from '@/features/butler/viewmodels/butlerPhase';

/** Structural ref type — works with both React 18 and 19 useRef typings. */
export type Ref<T> = { current: T };

export type PresentSpec = NonNullable<ReturnType<typeof extractPresentSpec>['spec']>;

export interface SinkDeps {
  isCancelled: () => boolean;
  setState: Dispatch<SetStateAction<ButlerState>>;
  markDiagramReady: () => void;
  analyser: AnalyserNode;
  playAnalyser: AnalyserNode;
  analyserRef: Ref<AnalyserNode | null>;
  heardRef: Ref<string>;
  fullReplyRef: Ref<string>;
  prevPhaseRef: Ref<string>;
  specShownRef: Ref<boolean>;
  visualGateRef: Ref<VisualReadyGate | null>;
  visualDecisionRef: Ref<boolean>;
  visualExpectedRef: Ref<boolean>;
  /** Is the globe/map currently centre-stage? While it is, a follow-up may
   *  name a new place without repeating an explicit cue ("what about Tokyo"). */
  mapOpenRef: Ref<boolean>;
  /** Leave Butler mode — "exit butler mode" said aloud should actually work,
   *  since reaching for the mouse defeats a hands-free surface. */
  onExit: () => void;
}

export function createButlerSinks(deps: SinkDeps): CallSinks {
  const {
    isCancelled,
    setState,
    markDiagramReady,
    heardRef,
    fullReplyRef,
    specShownRef,
    visualGateRef,
    visualDecisionRef,
    visualExpectedRef,
    mapOpenRef,
    onExit,
  } = deps;

  // STRICT (explicit cue only) — this decides whether a place ask OWNS the
  // stage, so it must never fire on an explainer. The loose follow-up form is
  // for moving an already-open map and nothing else: it accepts any short noun
  // ("show me photosynthesis"), which as a stage-owner silently suppressed the
  // diagram while the geocode correctly found no such place — so neither the
  // map nor the diagram appeared. See the async resolve in setHeard.
  const placeOwnsStage = () => hasPlaceCandidate(heardRef.current);

  const showDiagram = (spec: PresentSpec) => {
    if (isCancelled() || placeOwnsStage()) return;
    specShownRef.current = true;
    setState((s) => ({
      ...s,
      presentation: nextPresentation(s.presentation, { type: 'diagram', spec }),
    }));
    void fetchTopicImage(spec.title).then((item) => {
      if (item && !isCancelled()) setState((s) => ({ ...s, content: [item] }));
    });
  };

  return {
    setPhase: makeSetPhase(deps),
    // Nothing to render: the filler is audio the caller hears, and showing it
    // as text would put "let me check" on screen as if it were the answer.
    // It exists on the wire purely so the echo veto knows those words are his.
    setFiller: () => undefined,
    setHeard: (heard) => {
      if (isCancelled()) return;
      // Read BEFORE this turn can change the stage: what matters is whether
      // the caller was already looking at a map when they said this.
      const mapOpen = mapOpenRef.current;
      // "Exit butler mode" said aloud leaves Butler mode. Handled before any
      // turn bookkeeping: the surface is going away, so raising a diagram or
      // geocoding a place for it would be wasted work (and unmounting cancels
      // the in-flight turn anyway).
      if (isExitButlerCommand(heard)) {
        onExit();
        return;
      }
      heardRef.current = heard;
      specShownRef.current = false; // new turn — allow the next spec to raise
      visualGateRef.current?.release();
      visualExpectedRef.current = expectsVisualExplanation(heard);
      visualGateRef.current = visualExpectedRef.current ? createVisualReadyGate() : null;
      visualDecisionRef.current = false;
      setState((s) => ({ ...s, heard, log: [...s.log, { who: 'you', text: heard }] }));
      // No placeholder diagram while thinking: a generic scaffold titled
      // with the raw request reads as a wrong finished answer. The stage
      // stays on voice until a REAL diagram is ready — the model's own spec
      // (setReply) or the reply-content fallback at turn end (finalizeVisual).
      // Raise the globe as you speak — but only once a candidate actually
      // geocodes, so "where's my file" never opens a map.
      void (async () => {
        const places = await resolvePlaces(heard, mapOpen);
        if (!isCancelled() && places.length > 0) {
          setState((s) => ({ ...s, presentation: nextPresentation(s.presentation, { type: 'places', places }) }));
        }
      })();
    },
    setReply: (reply) => {
      if (isCancelled()) return;
      fullReplyRef.current = reply;
      const placeAsked = placeOwnsStage();
      const { spec } = extractPresentSpec(reply);
      // The server emits `reply` immediately before the corresponding audio
      // sentence. Therefore the first reply is the hard decision point: a
      // leading spec keeps/creates the gate until Mermaid is visible; no
      // spec (or a map-owned turn) releases it before normal speech.
      const firstDecision = !visualDecisionRef.current;
      if (firstDecision) {
        visualDecisionRef.current = true;
        if (spec && !placeAsked) {
          visualGateRef.current ??= createVisualReadyGate();
        } else {
          // No leading spec in the first reply. Don't stall the voice on the
          // gate's full timeout waiting for a diagram that isn't streaming —
          // release now so speech starts on time. A real diagram still comes:
          // the reply-content fallback raises one at turn end (finalizeVisual),
          // and a spec in a later chunk still shows via showDiagram below.
          markDiagramReady();
        }
      }
      // Caption drops a partial/complete spec block (no JSON flash).
      setState((s) => ({ ...s, reply: stripPresentTail(reply) }));
      // Raise the diagram the instant its block finishes STREAMING — text
      // completes well before the TTS audio drains (which is when the turn
      // ends), so the diagram appears while Regent is still speaking rather
      // than after. Idempotent per turn via specShownRef.
      // A place question owns the stage (the map) — never let a diagram the
      // model volunteered alongside it hijack the globe.
      // ...and never let one hijack a turn that was only ever pleasantries.
      // Closing a call drew "You're welcome" → "Glad that helped" → "If
      // anything else comes up": a model will volunteer a spec for anything,
      // and nothing here asked whether the TURN deserved a picture.
      if (!specShownRef.current && !placeAsked && spec && !isConversationalTurn(heardRef.current)) {
        showDiagram(spec);
      }
    },
    setError: (error) => {
      if (!isCancelled()) setState((s) => ({ ...s, error }));
    },
    waitForVisual: () => visualGateRef.current?.wait() ?? Promise.resolve(),
    finalizeVisual: async () => {
      const placeAsked = placeOwnsStage();
      if (placeAsked) {
        markDiagramReady();
        return;
      }
      if (specShownRef.current) return;

      let spec = extractPresentSpec(fullReplyRef.current).spec;
      // The REQUEST's phrasing is no longer the authority on whether an answer
      // gets a picture. It used to be `visualExpectedRef`, the keyword regex —
      // so "walk me through the stages of mitosis" or "what are the parts of a
      // cell" matched nothing in that list and the fallback was never tried,
      // however structured the answer turned out to be. That is the reported
      // "sometimes there's no timeline or concept map".
      //
      // What decides now is the ANSWER: `fallbackPresentSpec` returns null
      // unless the reply yields real explanation points, so a genuinely
      // conversational turn still gets no diagram without needing a keyword
      // list to predict it in advance. Small talk stays excluded, because a
      // chatty reply to "how are you" can still look list-shaped.
      const answerCouldCarryOne = !isConversationalTurn(heardRef.current);
      if (!spec && answerCouldCarryOne) {
        spec = await recoverDiagramArtifact(fullReplyRef.current);
      }
      if (!spec && answerCouldCarryOne) {
        spec = fallbackPresentSpec(heardRef.current, stripPresentTail(fullReplyRef.current));
      }
      if (spec) {
        showDiagram(spec);
        return;
      }
      markDiagramReady();
      // No diagram this turn — the weak model emitted a tool call, plain
      // reasoning, or an empty reply. If a PRIOR turn's diagram is still on
      // the stage, yield it back to voice so a bare turn doesn't leave a
      // stale, unrelated diagram sitting there.
      if (!isCancelled()) {
        setState((s) =>
          s.presentation.kind === 'diagram'
            ? { ...s, presentation: nextPresentation(s.presentation, { type: 'voice' }) }
            : s,
        );
      }
    },
  };
}
