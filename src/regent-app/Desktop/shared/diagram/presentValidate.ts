// The trust gate for a model-authored diagram spec. INTAKE is lenient (node/step
// entries as strings or objects, alternate field names), but every value is
// capped in count and length here — off-shape or over-cap ⇒ null, and the caller
// renders nothing. Extraction lives in `presentSpec`; these are its validators.

import type {
  PresentBranch,
  PresentEdge,
  PresentItem,
  PresentJourneySection,
  PresentJourneyStep,
  PresentMessage,
  PresentNode,
  PresentPoint,
  PresentSlice,
  PresentSpec,
  PresentStep,
} from './presentSpec';

const CAP = {
  nodes: 16, edges: 24, steps: 12, items: 4, branches: 8, children: 8,
  slices: 8, messages: 16, sections: 6, points: 12, label: 160, title: 100,
} as const;

const capStr = (v: unknown, max: number): string | null =>
  typeof v === 'string' && v.trim().length >= 1 && v.length <= max ? v.trim() : null;

// A node may be a bare string ("Sunlight") or an object ({id?, label}). id
// defaults to the label when absent.
function coerceNodes(v: unknown): PresentNode[] | null {
  if (!Array.isArray(v) || v.length < 1 || v.length > CAP.nodes) return null;
  const out: PresentNode[] = [];
  for (const raw of v) {
    if (typeof raw === 'string') {
      const s = capStr(raw, CAP.label);
      if (!s) return null;
      out.push({ id: s, label: s });
    } else if (raw && typeof raw === 'object') {
      const o = raw as Record<string, unknown>;
      const label = capStr(o.label, CAP.label) ?? capStr(o.name, CAP.label) ?? capStr(o.id, CAP.label);
      if (!label) return null;
      out.push({ id: capStr(o.id, CAP.label) ?? label, label });
    } else return null;
  }
  return out;
}

// Dangling edges are dropped (not fatal); a missing edges array ⇒ none.
function coerceEdges(v: unknown, ids: Set<string>): PresentEdge[] {
  if (!Array.isArray(v)) return [];
  const out: PresentEdge[] = [];
  for (const raw of v.slice(0, CAP.edges)) {
    if (!raw || typeof raw !== 'object') continue;
    const o = raw as Record<string, unknown>;
    const from = capStr(o.from ?? o.source, CAP.label);
    const to = capStr(o.to ?? o.target, CAP.label);
    if (!from || !to || !ids.has(from) || !ids.has(to)) continue;
    const label = capStr(o.label, CAP.label);
    out.push({ from, to, ...(label ? { label } : {}) });
  }
  return out;
}

function coerceSteps(v: unknown): PresentStep[] | null {
  if (!Array.isArray(v) || v.length < 1 || v.length > CAP.steps) return null;
  const out: PresentStep[] = [];
  for (const raw of v) {
    if (typeof raw === 'string') {
      const s = capStr(raw, CAP.label);
      if (!s) return null;
      out.push({ label: s });
    } else if (raw && typeof raw === 'object') {
      const o = raw as Record<string, unknown>;
      const label = capStr(o.label, CAP.label) ?? capStr(o.title, CAP.label) ?? capStr(o.name, CAP.label);
      if (!label) return null;
      const detail = capStr(o.detail, CAP.label) ?? capStr(o.description, CAP.label);
      out.push({ label, ...(detail ? { detail } : {}) });
    } else return null;
  }
  return out;
}

function coerceItems(v: unknown): PresentItem[] | null {
  if (!Array.isArray(v) || v.length < 2 || v.length > CAP.items) return null;
  const out: PresentItem[] = [];
  for (const raw of v) {
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    const name = capStr(o.name, CAP.label) ?? capStr(o.title, CAP.label);
    const rawPoints = Array.isArray(o.points) ? o.points : Array.isArray(o.values) ? o.values : null;
    if (!name || !rawPoints || rawPoints.length < 1 || rawPoints.length > CAP.steps) return null;
    const points: string[] = [];
    for (const p of rawPoints) {
      const s = capStr(p, CAP.label);
      if (!s) return null;
      points.push(s);
    }
    out.push({ name, points });
  }
  return out;
}

// A branch may carry its children under children/points/items, as strings or
// {label|name} objects — accept them all.
function coerceBranches(v: unknown): PresentBranch[] | null {
  if (!Array.isArray(v) || v.length < 1 || v.length > CAP.branches) return null;
  const out: PresentBranch[] = [];
  for (const raw of v) {
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    const label = capStr(o.label, CAP.label) ?? capStr(o.name, CAP.label) ?? capStr(o.title, CAP.label);
    if (!label) return null;
    const kids = Array.isArray(o.children) ? o.children : Array.isArray(o.points) ? o.points : Array.isArray(o.items) ? o.items : [];
    const children: string[] = [];
    for (const c of kids.slice(0, CAP.children)) {
      const s = capStr(typeof c === 'object' && c ? (c as Record<string, unknown>).label ?? (c as Record<string, unknown>).name : c, CAP.label);
      if (s) children.push(s);
    }
    out.push({ label, children });
  }
  return out;
}

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const clamp01 = (v: number): number => Math.min(1, Math.max(0, v));

function coerceSlices(v: unknown): PresentSlice[] | null {
  if (!Array.isArray(v) || v.length < 2 || v.length > CAP.slices) return null;
  const out: PresentSlice[] = [];
  for (const raw of v) {
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    const name = capStr(o.name ?? o.label, CAP.label);
    const value = num(o.value ?? o.count ?? o.percent);
    if (!name || value === null || value < 0) return null;
    out.push({ name, value });
  }
  return out;
}

function coerceMessages(v: unknown): PresentMessage[] | null {
  if (!Array.isArray(v) || v.length < 1 || v.length > CAP.messages) return null;
  const out: PresentMessage[] = [];
  for (const raw of v) {
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    const from = capStr(o.from ?? o.source ?? o.actor, CAP.label);
    const to = capStr(o.to ?? o.target, CAP.label);
    if (!from || !to) return null;
    const text = capStr(o.text ?? o.label ?? o.message, CAP.label);
    out.push({ from, to, ...(text ? { text } : {}) });
  }
  return out;
}

function coerceSections(v: unknown): PresentJourneySection[] | null {
  if (!Array.isArray(v) || v.length < 1 || v.length > CAP.sections) return null;
  const out: PresentJourneySection[] = [];
  for (const raw of v) {
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    const name = capStr(o.name ?? o.label ?? o.title, CAP.label);
    if (!name) return null;
    const kids = Array.isArray(o.steps) ? o.steps : Array.isArray(o.tasks) ? o.tasks : [];
    const steps: PresentJourneyStep[] = [];
    for (const s of kids.slice(0, CAP.steps)) {
      if (typeof s === 'string') steps.push({ label: s.slice(0, CAP.label), score: 3 });
      else if (s && typeof s === 'object') {
        const so = s as Record<string, unknown>;
        const label = capStr(so.label ?? so.name, CAP.label);
        if (label) steps.push({ label, score: Math.min(5, Math.max(1, Math.round(num(so.score ?? so.rating) ?? 3))) });
      }
    }
    if (steps.length > 0) out.push({ name, steps });
  }
  return out.length > 0 ? out : null;
}

function coercePoints(v: unknown): PresentPoint[] | null {
  if (!Array.isArray(v) || v.length < 1 || v.length > CAP.points) return null;
  const out: PresentPoint[] = [];
  for (const raw of v) {
    if (!raw || typeof raw !== 'object') return null;
    const o = raw as Record<string, unknown>;
    const label = capStr(o.label ?? o.name, CAP.label);
    const pos = Array.isArray(o.pos) ? o.pos : undefined;
    const x = num(o.x ?? pos?.[0]);
    const y = num(o.y ?? pos?.[1]);
    if (!label || x === null || y === null) return null;
    out.push({ label, x: clamp01(x), y: clamp01(y) });
  }
  return out;
}

const axis = (v: unknown, lo: string, hi: string): [string, string] => {
  const a = Array.isArray(v) ? v : [];
  return [capStr(a[0], 40) ?? lo, capStr(a[1], 40) ?? hi];
};

export function validate(raw: unknown): PresentSpec | null {
  if (typeof raw !== 'object' || raw === null) return null;
  const o = raw as Record<string, unknown>;
  const type = typeof o.type === 'string' ? o.type.toLowerCase().replace(/[\s_-]/g, '') : '';
  const title = capStr(o.title, CAP.title) ?? capStr(o.name, CAP.title) ?? 'Overview';
  if (type === 'flow' || type === 'concept') {
    const ns = coerceNodes(o.nodes);
    if (!ns) return null;
    return { type, title, nodes: ns, edges: coerceEdges(o.edges, new Set(ns.map((n) => n.id))) };
  }
  if (type === 'cycle') {
    const ns = coerceNodes(o.nodes ?? o.steps);
    return ns && ns.length >= 2 ? { type: 'cycle', title, nodes: ns } : null;
  }
  if (type === 'timeline') {
    const steps = coerceSteps(o.steps ?? o.events);
    return steps ? { type: 'timeline', title, steps } : null;
  }
  if (type === 'compare' || type === 'comparison') {
    const items = coerceItems(o.items);
    return items ? { type: 'compare', title, items } : null;
  }
  if (type === 'mindmap') {
    const branches = coerceBranches(o.branches ?? o.nodes ?? o.items);
    return branches ? { type: 'mindmap', title, branches } : null;
  }
  if (type === 'pie') {
    const slices = coerceSlices(o.slices ?? o.items ?? o.data);
    return slices ? { type: 'pie', title, slices } : null;
  }
  if (type === 'sequence') {
    const messages = coerceMessages(o.messages ?? o.steps);
    return messages ? { type: 'sequence', title, messages } : null;
  }
  if (type === 'journey') {
    const sections = coerceSections(o.sections ?? o.stages);
    return sections ? { type: 'journey', title, sections } : null;
  }
  if (type === 'quadrant') {
    const points = coercePoints(o.points ?? o.items);
    return points
      ? { type: 'quadrant', title, xAxis: axis(o.xAxis ?? o.x_axis, 'Low', 'High'), yAxis: axis(o.yAxis ?? o.y_axis, 'Low', 'High'), points }
      : null;
  }
  return null;
}
