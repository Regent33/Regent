// Coordinates a Butler turn's first audio with its visual explanation. The
// gate is framework-free so the streaming loop can await it while React and
// Mermaid render independently. The timeout is only a deadlock fuse.

import type { PresentSpec } from '@/shared/diagram/presentSpec';
import { explanationPoints } from '@/features/butler/domain/explanationPoints';

export interface VisualReadyGate {
  readonly wait: () => Promise<void>;
  readonly release: () => void;
}

export function createVisualReadyGate(timeoutMs = 12_000): VisualReadyGate {
  let released = false;
  let resolve!: () => void;
  const ready = new Promise<void>((done) => {
    resolve = done;
  });
  const timeout = globalThis.setTimeout(() => {
    if (!released) {
      released = true;
      resolve();
    }
  }, timeoutMs);

  return {
    wait: () => ready,
    release: () => {
      if (released) return;
      released = true;
      globalThis.clearTimeout(timeout);
      resolve();
    },
  };
}

/** Requests where a visual-first answer is especially important. The model
 * may still volunteer a diagram for any other request; this early signal only
 * holds a slow-turn filler until the first real reply makes that decision. */
// Greetings / small talk collide with the explainer cues below ("how are you"
// trips "how are…"), but they want plain speech, not a diagram. The OBJECT
// disambiguates: "how are YOU / things / everyone" is a greeting; "how are these
// RELATED" is a real ask — so only the greeting objects short-circuit, and the
// `^` anchor keeps a greeting buried mid-request from suppressing it.
const SMALL_TALK =
  /^(?:hi|hey+|hello|yo|hiya|howdy|sup|greetings|good (?:morning|afternoon|evening|day)|how (?:are|is|r) (?:you|u|ya|things|it going|everyone|everything)|how (?:you|ya|u) (?:doing|been)|how'?s it going|how do you do|how have you been|nice to (?:meet|see) you|long time no see)\b/i;

/** Greetings and pleasantries: they want speech, not a picture.
 *
 * Exported because the LAST-RESORT visual needs this suppression WITHOUT
 * inheriting the keyword list below. The keywords are a good early signal for
 * holding a filler, but they are a bad authority on whether a finished answer
 * deserves a diagram: "walk me through the stages of mitosis" and "what are the
 * parts of a cell" match nothing in that list, so the fallback was never even
 * attempted and a perfectly structured explanation came out as prose. The
 * fallback already self-gates — it returns null unless the REPLY has real
 * explanation points — so letting the content decide is both simpler and
 * strictly more correct. This keeps "hey, how are you" out of it. */
export function isSmallTalk(heard: string): boolean {
  const said = heard.trim();
  if (!SMALL_TALK.test(said)) return false;
  // A greeting is small talk only when nothing substantive follows it. The `^`
  // anchor alone made "hey, explain the history of Rome" small talk, and callers
  // read that as "this turn wants speech" — so the model's own spec was vetoed
  // (butlerSinks.ts:158), the last-resort visual never ran (butlerSinks.ts:187),
  // and the filler cue was suppressed too (expectsVisualExplanation below).
  // Callers address this surface as "hey Regent, ...", so the greeting-prefixed
  // real request is the COMMON spoken shape, not the edge case.
  // "there" belongs to the greeting it follows ("hey there", "hi there"), but
  // SMALL_TALK matches only the greeting token, so it would otherwise count as
  // the first word of a "real request".
  const rest = said
    .replace(SMALL_TALK, '')
    .replace(/^[\s,.!?-]+/, '')
    .replace(/^there\b[\s,.!?-]*/i, '');
  if (rest === '') return true;
  // `replace` strips only the FIRST matching alternative, so "hey, how are you"
  // leaves "how are you" — three words, which a bare word-count test calls a
  // real request. That is the single most common voice opener there is, and it
  // would hold the audio behind the visual gate. Recurse instead: the remainder
  // of a greeting is very often another greeting, and it always shrinks, so
  // this terminates.
  return rest.split(/\s+/).filter(Boolean).length <= 2 || isSmallTalk(rest) || isPleasantry(rest);
}

/** Words a turn made ENTIRELY of pleasantries can be built from. */
const PLEASANTRY_WORDS =
  /^(?:ok|okay|alright|cool|nice|great|perfect|awesome|lovely|thanks|thank|you|ty|got|it|understood|makes|sense|never|mind|nvm|bye|goodbye|see|ya|later|good|night|that|s|all|is|much|lot|so|very|again|no|problem|yeah|yep|yes|sure|right|fine|helpful|appreciate|cheers)$/;

/** True when the whole utterance is thanks or a sign-off — "okay, thank you.
 * Thank you", "got it, cheers".
 *
 * Field report, with screenshots: closing a call produced a flowchart reading
 * "You're welcome" → "Glad that helped" → "If anything else comes up". A model
 * will happily volunteer a spec for a turn like that, and nothing checked
 * whether the TURN deserved a picture before putting it on the stage.
 *
 * Every word must be a pleasantry, so "great, now explain the stages" is not
 * one — a prefix match on "great" would have swallowed a real request. Bounded
 * in length because a long message containing only these words is a sentence
 * doing something else. */
export function isPleasantry(heard: string): boolean {
  const words = heard
    .toLowerCase()
    .replace(/[^a-z\s]/g, ' ')
    .split(/\s+/)
    .filter(Boolean);
  return words.length > 0 && words.length <= 8 && words.every((w) => PLEASANTRY_WORDS.test(w));
}

/** A turn that wants speech, never a diagram: greetings and sign-offs. */
export function isConversationalTurn(heard: string): boolean {
  return isSmallTalk(heard) || isPleasantry(heard);
}

export function expectsVisualExplanation(heard: string): boolean {
  if (isSmallTalk(heard)) return false;
  return /\b(?:diagram|visuali[sz]e|explain|teach|walk me through|tell me about|why|compare|comparison|versus|vs\.?|difference|different|how (?:does|do|is|are)|process|workflow|flow|steps?|history|chronology|timeline|sequence|cycle|overview|architecture|relationship|break ?down|pros and cons|proportion|percentage|distribution|matrix|journey|interaction|concept map)\b/i.test(
    heard,
  );
}

type VisualType = PresentSpec['type'];

/** Deterministic last-resort visual. Models normally provide the richer spec,
 * but an explainer must never become prose-only because a weaker provider
 * omitted the inline block or wrote it to an artifact. */
export function fallbackPresentSpec(heard: string, reply: string): PresentSpec | null {
  // A one-word turn carrying no question is a backchannel — "oh", "hmm", "wow",
  // "really". Field report: "oh" alone produced a two-node flowchart. A word
  // list is what let it through (every report adds a word, the next backchannel
  // slips past), so the bar is the SHAPE of the turn instead.
  // ponytail: raise to <=2 words if "oh wow" shows up in the field — at the cost
  // of "explain mitosis". A model that genuinely wants a visual for a short turn
  // can still volunteer its own spec; this gates only the deterministic override.
  if (heard.trim().split(/\s+/).filter(Boolean).length <= 1 && !heard.includes('?')) return null;
  const points = explanationPoints(reply);
  // One point is not an explanation. The old floor invented a second node
  // labelled 'Result' to reach a drawable shape — a fabricated label is the
  // signature of this fallback firing on something that was never an
  // explanation, and it can never be correct.
  if (points.length < 2) return null;
  const title = visualTitle(heard);
  const type = visualTypeFor(heard);
  const labels = points;

  switch (type) {
    case 'timeline':
      return { type, title, steps: labels.map((label) => ({ label })) };
    case 'compare': {
      const names = comparisonNames(heard);
      return {
        type,
        title,
        items: names.map((name, index) => ({
          name,
          points: labels.filter((_, pointIndex) => pointIndex % names.length === index).slice(0, 4),
        })).map((item) => ({ ...item, points: item.points.length > 0 ? item.points : [labels[0]] })),
      };
    }
    case 'mindmap':
      return {
        type,
        title,
        branches: labels.slice(0, 6).map((label, index) => ({
          label,
          children: labels[index + 1] ? [labels[index + 1]] : [],
        })),
      };
    case 'pie':
      return {
        type,
        title,
        slices: labels.slice(0, 8).map((name) => ({ name, value: 1 })),
      };
    case 'sequence':
      return {
        type,
        title,
        messages: labels.slice(0, 10).map((text, index) => ({
          from: index % 2 === 0 ? 'Actor A' : 'Actor B',
          to: index % 2 === 0 ? 'Actor B' : 'Actor A',
          text,
        })),
      };
    case 'journey':
      return {
        type,
        title,
        sections: [{ name: 'Stages', steps: labels.slice(0, 10).map((label) => ({ label, score: 3 })) }],
      };
    case 'quadrant':
      return {
        type,
        title,
        xAxis: ['Low effort', 'High effort'],
        yAxis: ['Low impact', 'High impact'],
        points: labels.slice(0, 10).map((label, index, all) => ({
          label,
          x: all.length === 1 ? 0.5 : index / (all.length - 1),
          y: 1 - index / Math.max(1, all.length),
        })),
      };
    case 'cycle':
      return {
        type,
        title,
        nodes: labels.slice(0, 10).map((label, index) => ({ id: `n${index + 1}`, label })),
      };
    case 'concept': {
      const nodes = labels.slice(0, 10).map((label, index) => ({ id: `n${index + 1}`, label }));
      return {
        type,
        title,
        nodes,
        edges: nodes.slice(1).map((node) => ({ from: nodes[0].id, to: node.id })),
      };
    }
    case 'flow': {
      const nodes = labels.slice(0, 10).map((label, index) => ({ id: `n${index + 1}`, label }));
      return {
        type,
        title,
        nodes,
        edges: nodes.slice(1).map((node, index) => ({ from: nodes[index].id, to: node.id })),
      };
    }
  }
}

function visualTypeFor(heard: string): VisualType {
  if (/\b(?:history|chronology|timeline|over time)\b/i.test(heard)) return 'timeline';
  if (/\b(?:compare|comparison|versus|vs\.?|difference|pros and cons)\b/i.test(heard)) return 'compare';
  if (/\b(?:cycle|loop|repeating|recurring)\b/i.test(heard)) return 'cycle';
  if (/\b(?:sequence|interaction|message exchange|conversation)\b/i.test(heard)) return 'sequence';
  if (/\b(?:journey|experience|user flow)\b/i.test(heard)) return 'journey';
  if (/\b(?:proportion|percentage|share|distribution|breakdown by)\b/i.test(heard)) return 'pie';
  if (/\b(?:quadrant|matrix|effort (?:and|vs\.?) impact|positioning)\b/i.test(heard)) return 'quadrant';
  if (/\b(?:relationship|concept map|connections?)\b/i.test(heard)) return 'concept';
  if (/\b(?:overview|break ?down|parts|components|tell me about)\b/i.test(heard)) return 'mindmap';
  return 'flow';
}


function visualTitle(heard: string): string {
  const cleaned = heard
    .replace(/\b(?:can|could|would) you\b/gi, '')
    .replace(/\b(?:please|explain|show me|visualize|compare|tell me about|walk me through)\b/gi, '')
    .replace(/[?!.]+$/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  const title = cleaned === '' ? 'Explanation' : cleaned;
  return title.length > 72 ? `${title.slice(0, 69)}…` : title;
}

function comparisonNames(heard: string): [string, string] {
  const parts = heard
    .replace(/.*?\b(?:compare|comparison of)\b/i, '')
    .split(/\b(?:versus|vs\.?)\b/i)
    .map((part) => part.replace(/[?!.]+$/g, '').trim())
    .filter(Boolean);
  return [parts[0] || 'Option A', parts[1] || 'Option B'];
}
