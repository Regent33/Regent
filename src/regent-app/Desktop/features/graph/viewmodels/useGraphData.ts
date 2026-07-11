'use client';
// Loads the memory graph once on mount via `memory.graph` and shapes it for
// the galaxy view: validated nodes/edges, a per-node degree (incident-edge
// count, drives dot size), plus stable kind→colour/glyph helpers. Outside the
// desktop shell the RPC fails typed, so we degrade to an empty graph.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';

export interface GraphNode {
  readonly id: string;
  readonly kind: string;
  readonly name: string;
  readonly content: string;
  readonly pinned: boolean;
  readonly degree: number;
}

export interface GraphEdge {
  readonly src: string;
  readonly dst: string;
  readonly relation: string;
  readonly weight: number;
}

export interface GraphDataState {
  readonly nodes: readonly GraphNode[];
  readonly edges: readonly GraphEdge[];
  readonly loading: boolean;
  readonly error?: string;
}

interface RawGraph {
  readonly nodes: readonly Omit<GraphNode, 'degree'>[];
  readonly edges: readonly GraphEdge[];
}

// Eight calm, saturated hues that all read on the near-black space field.
const PALETTE = [
  '#7aa2f7', '#bb9af7', '#7dcfff', '#9ece6a',
  '#e0af68', '#f7768e', '#2ac3de', '#ff9e64',
] as const;

// Intentional colours/glyphs for the kinds graph memory emits today; any other
// kind falls through to a hashed palette slot + its initial, so the set stays
// open without a code change.
const KNOWN_COLOR: Record<string, string> = {
  user: '#7dcfff', fact: '#7aa2f7', memory: '#bb9af7',
  entity: '#9ece6a', preference: '#e0af68', goal: '#f7768e',
};
const KNOWN_GLYPH: Record<string, string> = {
  user: '@', fact: '·', memory: '✦', entity: '◆', preference: '★', goal: '⚑',
};

/** djb2 — a tiny stable string hash so an unknown kind always lands on the
 * same palette slot across renders and sessions. */
function hash(s: string): number {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  return Math.abs(h);
}

export function kindColor(kind: string): string {
  return KNOWN_COLOR[kind] ?? PALETTE[hash(kind) % PALETTE.length];
}

export function kindGlyph(kind: string): string {
  return KNOWN_GLYPH[kind] ?? (kind.charAt(0).toUpperCase() || '?');
}

/** Incident-edge count per node id — both endpoints count. Pure, so the test
 * can exercise it without the hook. */
export function computeDegrees(
  nodes: readonly { id: string }[],
  edges: readonly GraphEdge[],
): Map<string, number> {
  const deg = new Map<string, number>();
  for (const n of nodes) deg.set(n.id, 0);
  for (const e of edges) {
    if (deg.has(e.src)) deg.set(e.src, (deg.get(e.src) ?? 0) + 1);
    if (deg.has(e.dst)) deg.set(e.dst, (deg.get(e.dst) ?? 0) + 1);
  }
  return deg;
}

function str(v: unknown, fallback = ''): string {
  return typeof v === 'string' ? v : fallback;
}

function toRaw(value: unknown): RawGraph {
  const v = (value ?? {}) as Record<string, unknown>;
  const rawNodes = Array.isArray(v.nodes) ? v.nodes : [];
  const rawEdges = Array.isArray(v.edges) ? v.edges : [];
  const nodes = rawNodes
    .map((n) => (typeof n === 'object' && n !== null ? (n as Record<string, unknown>) : undefined))
    .filter((n): n is Record<string, unknown> => n !== undefined && typeof n.id === 'string')
    .map((n) => ({
      id: n.id as string,
      kind: str(n.kind, 'memory'),
      name: str(n.name) || (n.id as string),
      content: str(n.content),
      pinned: n.pinned === true,
    }));
  const edges = rawEdges
    .map((e) => (typeof e === 'object' && e !== null ? (e as Record<string, unknown>) : undefined))
    .filter((e): e is Record<string, unknown> => e !== undefined && typeof e.src === 'string' && typeof e.dst === 'string')
    .map((e) => ({
      src: e.src as string,
      dst: e.dst as string,
      relation: str(e.relation),
      weight: typeof e.weight === 'number' ? e.weight : 1,
    }));
  return { nodes, edges };
}

export function useGraphData(): GraphDataState {
  const [state, setState] = useState<GraphDataState>({ nodes: [], edges: [], loading: true });

  useEffect(() => {
    if (!isTauri()) {
      setState({ nodes: [], edges: [], loading: false });
      return;
    }
    let alive = true;
    void deaconRequest('memory.graph', { limit: 2000 }).then((result) => {
      if (!alive) return;
      if (!result.ok) {
        setState({ nodes: [], edges: [], loading: false, error: result.error.message });
        return;
      }
      const { nodes: raw, edges } = toRaw(result.value);
      const deg = computeDegrees(raw, edges);
      const nodes = raw.map((n) => ({ ...n, degree: deg.get(n.id) ?? 0 }));
      setState({ nodes, edges, loading: false });
    });
    return () => {
      alive = false;
    };
  }, []);

  return state;
}
