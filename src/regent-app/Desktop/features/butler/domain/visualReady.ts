// Coordinates a Butler turn's first audio with its visual explanation. The
// gate is framework-free so the streaming loop can await it while React and
// Mermaid render independently. The timeout is only a deadlock fuse.

import type { PresentSpec } from '@/shared/diagram/presentSpec';

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
export function expectsVisualExplanation(heard: string): boolean {
  return /\b(?:diagram|visuali[sz]e|explain|teach|walk me through|tell me about|why|compare|comparison|versus|vs\.?|difference|different|how (?:does|do|is|are)|process|workflow|flow|steps?|history|chronology|timeline|sequence|cycle|overview|architecture|relationship|break ?down|pros and cons|proportion|percentage|distribution|matrix|journey|interaction|concept map)\b/i.test(
    heard,
  );
}

type VisualType = PresentSpec['type'];

/** Deterministic last-resort visual. Models normally provide the richer spec,
 * but an explainer must never become prose-only because a weaker provider
 * omitted the inline block or wrote it to an artifact. */
export function fallbackPresentSpec(heard: string, reply: string): PresentSpec | null {
  const points = explanationPoints(reply);
  if (points.length === 0) return null;
  const title = visualTitle(heard);
  const type = visualTypeFor(heard);
  const labels = points.length === 1 ? [points[0], 'Result'] : points;

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

function explanationPoints(reply: string): string[] {
  const cleaned = reply
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/https?:\/\/\S+/g, ' ')
    .replace(/^\s*[-*#>]+\s*/gm, '')
    .replace(/\s+/g, ' ')
    .trim();
  if (cleaned === '') return [];
  return cleaned
    .split(/(?<=[.!?;:])\s+/)
    .map((part) => part.replace(/[.!?;:]+$/, '').trim())
    .filter((part) => part.length >= 3)
    .slice(0, 8)
    .map((part) => (part.length > 72 ? `${part.slice(0, 69)}…` : part));
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
