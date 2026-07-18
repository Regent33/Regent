// The diagram spec Regent may append to a butler reply. This module is a TRUST
// BOUNDARY: the block is model-authored JSON, so structure is bounded (known
// type, length + count caps in `presentValidate`). But EXTRACTION is lenient —
// voice models emit the spec in many shapes (```present, ```json, a bare
// trailing {…}) — so we accept whatever is roughly right and let the caps be the
// safety gate. Off-shape → null spec; the caption/log still get the cleaned
// prose. `stripPresentTail` keeps a half-streamed block out of the live caption.

import { validate } from './presentValidate';

export type PresentNode = { id: string; label: string };
export type PresentEdge = { from: string; to: string; label?: string };
export type PresentStep = { label: string; detail?: string };
export type PresentItem = { name: string; points: string[] };
export type PresentBranch = { label: string; children: string[] };
export type PresentSlice = { name: string; value: number };
export type PresentMessage = { from: string; to: string; text?: string };
export type PresentJourneyStep = { label: string; score: number };
export type PresentJourneySection = { name: string; steps: PresentJourneyStep[] };
export type PresentPoint = { label: string; x: number; y: number };

export type PresentSpec =
  | { type: 'flow'; title: string; nodes: PresentNode[]; edges: PresentEdge[] }
  | { type: 'concept'; title: string; nodes: PresentNode[]; edges: PresentEdge[] }
  | { type: 'cycle'; title: string; nodes: PresentNode[] }
  | { type: 'timeline'; title: string; steps: PresentStep[] }
  | { type: 'compare'; title: string; items: PresentItem[] }
  | { type: 'mindmap'; title: string; branches: PresentBranch[] }
  | { type: 'pie'; title: string; slices: PresentSlice[] }
  | { type: 'sequence'; title: string; messages: PresentMessage[] }
  | { type: 'journey'; title: string; sections: PresentJourneySection[] }
  | { type: 'quadrant'; title: string; xAxis: [string, string]; yAxis: [string, string]; points: PresentPoint[] };

// Every fenced block, whatever the language tag (```present / ```json / ```).
const FENCE_RE = /```[a-zA-Z]*[ \t]*\r?\n?([\s\S]*?)```/g;

/** Pull the diagram spec out of a finished reply. Tries each fenced block (last
 * first — the spec goes at the end) then a bare trailing JSON object; the strict
 * validator gates, so a real code block simply won't parse as a spec. Returns
 * the spec (or null) and the reply with that block removed. */
export function extractPresentSpec(reply: string): { spec: PresentSpec | null; text: string } {
  const blocks: Array<{ start: number; end: number; body: string }> = [];
  FENCE_RE.lastIndex = 0;
  for (let m = FENCE_RE.exec(reply); m !== null; m = FENCE_RE.exec(reply)) {
    blocks.push({ start: m.index, end: m.index + m[0].length, body: m[1] });
  }
  for (let i = blocks.length - 1; i >= 0; i--) {
    const spec = tryParse(blocks[i].body);
    if (spec) {
      const text = (reply.slice(0, blocks[i].start) + reply.slice(blocks[i].end)).replace(/\s+$/, '');
      return { spec, text };
    }
  }
  // A bare trailing JSON object (no fence) carrying a "type" field.
  const bare = /(\{[\s\S]*\})\s*$/.exec(reply);
  if (bare && bare[1].includes('"type"')) {
    const spec = tryParse(bare[1]);
    if (spec) return { spec, text: reply.slice(0, bare.index).replace(/\s+$/, '') };
  }
  return { spec: null, text: reply };
}

function tryParse(body: string): PresentSpec | null {
  const parsed = parseFirstObject(body);
  return parsed === undefined ? null : validate(parsed);
}

/** Parse the FIRST complete JSON object out of `body`, tolerating trailing junk
 * a model sometimes appends inside the fence — a duplicate `}`, a stray comma,
 * or a sentence after the spec. A strict `JSON.parse` rejects the whole block
 * for one trailing character, which silently drops an otherwise-perfect diagram
 * (observed: a valid timeline followed by an extra `}` rendered nothing). The
 * strict `validate` below is still the trust gate; this only widens INTAKE. */
function parseFirstObject(body: string): unknown {
  const s = body.trim();
  try {
    return JSON.parse(s); // fast path: clean JSON
  } catch {
    // fall through to a brace-balanced scan of the first object
  }
  const start = s.indexOf('{');
  if (start === -1) return undefined;
  let depth = 0;
  let inStr = false;
  let esc = false;
  for (let i = start; i < s.length; i++) {
    const ch = s[i];
    if (inStr) {
      if (esc) esc = false;
      else if (ch === '\\') esc = true;
      else if (ch === '"') inStr = false;
    } else if (ch === '"') inStr = true;
    else if (ch === '{') depth += 1;
    else if (ch === '}' && --depth === 0) {
      try {
        return JSON.parse(s.slice(start, i + 1));
      } catch {
        return undefined;
      }
    }
  }
  return undefined; // never closed → genuinely broken
}

/** For the STREAMING caption: cut everything from a partial or complete spec
 * block onward, so half-written JSON never shows mid-stream. */
export function stripPresentTail(live: string): string {
  const cut = (i: number) => live.slice(0, i).replace(/\s+$/, '');
  // The spec now LEADS the reply: once its fence has closed, drop just the
  // block and show the prose that follows. While it's still streaming (no
  // closing fence yet) this won't match and the tail logic below blanks the
  // caption, so half-written JSON never shows. Gated on a "type" field so an
  // ordinary leading code block isn't mistaken for a spec.
  const lead = /^\s*```(?:present|json)?[ \t]*\r?\n([\s\S]*?)```[ \t]*\r?\n?/i.exec(live);
  if (lead && /"type"/.test(lead[1])) return live.slice(lead[0].length).replace(/^\s+/, '');
  // A labelled spec fence (```present / ```json), still open (or trailing).
  const labelled = live.search(/```(?:present|json)\b/i);
  if (labelled !== -1) return cut(labelled);
  // A trailing fence whose label is still arriving and prefixes a spec label
  // (bare ``` or ```pres…) — but NOT a settled non-spec label like ```bash.
  const partial = /```([a-z]*)$/i.exec(live);
  if (partial) {
    const lang = partial[1].toLowerCase();
    if ('present'.startsWith(lang) || 'json'.startsWith(lang)) return cut(partial.index);
  }
  // A bare trailing JSON object that has begun declaring a "type".
  const brace = /\{[\s\S]*$/.exec(live);
  if (brace && /"type"/.test(brace[0])) return cut(brace.index);
  return live;
}
