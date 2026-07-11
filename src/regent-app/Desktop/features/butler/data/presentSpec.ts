// The ```present block Regent may append to a butler reply — a small declarative
// diagram spec. This module is a TRUST BOUNDARY: the block is model-authored
// JSON, so parsing is STRICT (known type, required arrays present, length + string
// caps, edges must reference real node ids). Anything off-shape yields a null
// spec; the caption/log still get the cleaned prose. `stripPresentTail` keeps a
// half-streamed block from ever flashing in the live caption.

export type PresentNode = { id: string; label: string };
export type PresentEdge = { from: string; to: string; label?: string };
export type PresentStep = { label: string; detail?: string };
export type PresentItem = { name: string; points: string[] };

export type PresentSpec =
  | { type: 'flow'; title: string; nodes: PresentNode[]; edges: PresentEdge[] }
  | { type: 'concept'; title: string; nodes: PresentNode[]; edges: PresentEdge[] }
  | { type: 'timeline'; title: string; steps: PresentStep[] }
  | { type: 'compare'; title: string; items: PresentItem[] };

const CAP = { nodes: 16, edges: 24, steps: 12, items: 4, label: 120, title: 80 } as const;

const PRESENT_RE = /```present\s*([\s\S]*?)```/;

/** Pull the diagram spec out of a finished reply. Returns the validated spec
 * (or null) and the reply with the block removed (for captions and the log). */
export function extractPresentSpec(reply: string): { spec: PresentSpec | null; text: string } {
  const m = PRESENT_RE.exec(reply);
  if (!m) return { spec: null, text: reply };
  const text = (reply.slice(0, m.index) + reply.slice(m.index + m[0].length)).replace(/\s+$/, '');
  let spec: PresentSpec | null = null;
  try {
    spec = validate(JSON.parse(m[1]) as unknown);
  } catch {
    spec = null;
  }
  return { spec, text };
}

/** For the STREAMING caption: cut everything from a partial or complete
 * `present` fence onward, so half-written JSON never shows mid-stream. */
export function stripPresentTail(live: string): string {
  const labelled = live.indexOf('```present');
  if (labelled !== -1) return live.slice(0, labelled).replace(/\s+$/, '');
  // A fence whose label is still arriving: ``` + a prefix of "present".
  const m = /```([a-z]*)$/i.exec(live);
  if (m && 'present'.startsWith(m[1].toLowerCase())) return live.slice(0, m.index).replace(/\s+$/, '');
  return live;
}

function str(v: unknown, max: number): v is string {
  return typeof v === 'string' && v.length >= 1 && v.length <= max;
}

function arr(v: unknown, min: number, max: number): v is unknown[] {
  return Array.isArray(v) && v.length >= min && v.length <= max;
}

function nodes(v: unknown): PresentNode[] | null {
  if (!arr(v, 1, CAP.nodes)) return null;
  const out: PresentNode[] = [];
  for (const n of v) {
    const o = n as Record<string, unknown>;
    if (!str(o.id, CAP.label) || !str(o.label, CAP.label)) return null;
    out.push({ id: o.id, label: o.label });
  }
  return out;
}

function edges(v: unknown, ids: Set<string>): PresentEdge[] | null {
  if (!arr(v, 0, CAP.edges)) return null;
  const out: PresentEdge[] = [];
  for (const e of v) {
    const o = e as Record<string, unknown>;
    if (!str(o.from, CAP.label) || !str(o.to, CAP.label)) return null;
    if (!ids.has(o.from) || !ids.has(o.to)) return null; // dangling edge — reject
    if (o.label !== undefined && !str(o.label, CAP.label)) return null;
    out.push({ from: o.from, to: o.to, ...(o.label !== undefined ? { label: o.label as string } : {}) });
  }
  return out;
}

function validate(raw: unknown): PresentSpec | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const o = raw as Record<string, unknown>;
  if (!str(o.title, CAP.title)) return null;
  if (o.type === 'flow' || o.type === 'concept') {
    const ns = nodes(o.nodes);
    if (!ns) return null;
    const es = edges(o.edges, new Set(ns.map((n) => n.id)));
    if (!es) return null;
    return { type: o.type, title: o.title, nodes: ns, edges: es };
  }
  if (o.type === 'timeline') {
    if (!arr(o.steps, 1, CAP.steps)) return null;
    const steps: PresentStep[] = [];
    for (const s of o.steps) {
      const so = s as Record<string, unknown>;
      if (!str(so.label, CAP.label)) return null;
      if (so.detail !== undefined && !str(so.detail, CAP.label)) return null;
      steps.push({ label: so.label, ...(so.detail !== undefined ? { detail: so.detail as string } : {}) });
    }
    return { type: 'timeline', title: o.title, steps };
  }
  if (o.type === 'compare') {
    if (!arr(o.items, 2, CAP.items)) return null;
    const items: PresentItem[] = [];
    for (const it of o.items) {
      const io = it as Record<string, unknown>;
      if (!str(io.name, CAP.label)) return null;
      if (!arr(io.points, 1, CAP.steps)) return null;
      const points: string[] = [];
      for (const p of io.points) {
        if (!str(p, CAP.label)) return null;
        points.push(p);
      }
      items.push({ name: io.name, points });
    }
    return { type: 'compare', title: o.title, items };
  }
  return null; // unknown type
}
