'use client';
// d3-force simulation for the galaxy layout. Node positions live on the sim
// objects (d3 mutates x/y/vx/vy in place); we hand the canvas a ref to that
// mutable array so its rAF loop reads fresh positions every frame WITHOUT any
// React state update per tick — the whole point of a 1k-node view staying at
// 60fps. The sim decays to rest normally and is stopped on unmount.
import { useEffect, useRef } from 'react';
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from 'd3-force';
import type { GraphEdge, GraphNode } from './useGraphData';

export interface LayoutNode extends SimulationNodeDatum {
  id: string;
  kind: string;
  name: string;
  degree: number;
  radius: number;
}

interface LayoutLink extends SimulationLinkDatum<LayoutNode> {
  weight: number;
}

export interface ForceLayout {
  /** The live, mutable node array d3 ticks in place — the canvas reads it. */
  readonly nodesRef: React.RefObject<LayoutNode[]>;
  /** The running simulation, so the canvas can pin a dragged node (fx/fy) and
   * reheat (alphaTarget) — that's what makes the whole web spring elastically. */
  readonly simRef: React.RefObject<Simulation<LayoutNode, LayoutLink> | null>;
}

/** Dot radius grows with degree but flattens (sqrt) so hubs don't dwarf the
 * field. Shared with the canvas draw so hit-testing matches what's painted. */
export const nodeRadius = (degree: number): number => 9 + Math.sqrt(degree) * 5;

export function useForceLayout(
  nodes: readonly GraphNode[],
  edges: readonly GraphEdge[],
): ForceLayout {
  const layoutRef = useRef<LayoutNode[]>([]);
  const simRef = useRef<Simulation<LayoutNode, LayoutLink> | null>(null);

  useEffect(() => {
    const simNodes: LayoutNode[] = nodes.map((n) => ({
      id: n.id,
      kind: n.kind,
      name: n.name,
      degree: n.degree,
      radius: nodeRadius(n.degree),
    }));
    layoutRef.current = simNodes;
    if (simNodes.length === 0) return;

    const links: LayoutLink[] = edges.map((e) => ({ source: e.src, target: e.dst, weight: e.weight }));

    // Stronger charge + shorter, stiffer links pull connected nodes into visible
    // Obsidian-style clusters; collide keeps the bigger dots from overlapping.
    // A gentle velocityDecay makes a dragged node's neighbours glide (springy)
    // rather than snap, and the sim never fully freezes so it stays interactive.
    // Round-galaxy shape: charge repulsion is range-capped (unbounded
    // repulsion pushes disconnected islands apart forever — the graph
    // sprawled into an irregular archipelago), and weak x/y gravity pulls
    // every node — islands included — toward the origin. forceCenter can't do
    // that job: it only translates the centroid, it never compacts outliers.
    // Roundness comes from the force BALANCE: links must stay weak (soft
    // springs), so the isotropic pair — charge pushing out, gravity pulling
    // in — settles the hull into a circle. Stiff links (the old 0.5–1.0)
    // made the mesh's own triangles dictate an angular silhouette.
    const sim = forceSimulation<LayoutNode>(simNodes)
      .velocityDecay(0.28)
      .force('charge', forceManyBody<LayoutNode>().strength(-300).distanceMax(600))
      .force(
        'link',
        forceLink<LayoutNode, LayoutLink>(links)
          .id((d) => d.id)
          .distance((l) => 34 + 18 / Math.sqrt(Math.max(1, l.weight)))
          .strength((l) => Math.min(0.4, 0.18 + (l.weight ?? 1) * 0.04)),
      )
      .force('x', forceX<LayoutNode>(0).strength(0.09))
      .force('y', forceY<LayoutNode>(0).strength(0.09))
      .force('collide', forceCollide<LayoutNode>().radius((d) => d.radius + 4));
    simRef.current = sim;

    return () => {
      sim.stop();
      simRef.current = null;
    };
  }, [nodes, edges]);

  return { nodesRef: layoutRef, simRef };
}
