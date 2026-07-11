// Pure spec → mermaid source. Kept free of any mermaid import so it is trivially
// unit-testable and runnable off the render path. Every label is sanitized: the
// loader renders securityLevel 'strict' (safe injection), but backticks/quotes/
// braces in a label would still break the PARSE, so they are stripped here.
import type { PresentSpec } from '@/features/butler/data/presentSpec';

/** Strip characters that break mermaid parsing (quotes, backticks, brackets,
 * pipes, angle/curly braces, semicolons, hashes) and collapse whitespace. */
function esc(s: string): string {
  return s.replace(/[`"'<>|{}[\]()#;]/g, ' ').replace(/\s+/g, ' ').trim() || '·';
}

/** A mermaid-safe node id derived from an index (spec ids are arbitrary). */
function nid(i: number): string {
  return `n${i}`;
}

export function specToMermaid(spec: PresentSpec): string {
  switch (spec.type) {
    case 'flow':
    case 'concept': {
      const dir = spec.type === 'flow' ? 'TD' : 'LR';
      const order = new Map(spec.nodes.map((n, i) => [n.id, nid(i)]));
      const lines = [`flowchart ${dir}`];
      spec.nodes.forEach((n, i) => lines.push(`${nid(i)}["${esc(n.label)}"]`));
      for (const e of spec.edges) {
        const from = order.get(e.from);
        const to = order.get(e.to);
        if (!from || !to) continue;
        lines.push(e.label ? `${from} -->|"${esc(e.label)}"| ${to}` : `${from} --> ${to}`);
      }
      return lines.join('\n');
    }
    case 'timeline': {
      const lines = ['timeline', `  title ${esc(spec.title)}`];
      for (const s of spec.steps) {
        const label = esc(s.label);
        lines.push(`  ${label} : ${esc(s.detail ?? s.label)}`);
      }
      return lines.join('\n');
    }
    case 'compare': {
      const lines = ['flowchart TD'];
      spec.items.forEach((item, i) => {
        lines.push(`subgraph g${i}["${esc(item.name)}"]`);
        item.points.forEach((p, j) => lines.push(`  g${i}p${j}["${esc(p)}"]`));
        lines.push('end');
      });
      return lines.join('\n');
    }
  }
}
