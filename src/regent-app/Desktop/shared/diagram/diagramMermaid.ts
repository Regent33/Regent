// Pure spec → mermaid source. Kept free of any mermaid import so it is trivially
// unit-testable and runnable off the render path. Every label is sanitized: the
// loader renders securityLevel 'strict' (safe injection), but backticks/quotes/
// braces in a label would still break the PARSE, so they are stripped here.
// Nodes are colored from a rotating palette (via classDef) and the font is sized
// up through an init directive, so a diagram reads clearly and is never a wall
// of identical gray boxes.
import type { PresentSpec } from '@/shared/diagram/presentSpec';

// Vibrant fills that all read on the dark diagram backdrop; rounded via rx/ry.
const PALETTE = [
  'fill:#2563eb,stroke:#93c5fd,color:#ffffff', // blue
  'fill:#7c3aed,stroke:#c4b5fd,color:#ffffff', // violet
  'fill:#059669,stroke:#6ee7b7,color:#ffffff', // green
  'fill:#d97706,stroke:#fcd34d,color:#0b0b0b', // amber
  'fill:#db2777,stroke:#f9a8d4,color:#ffffff', // pink
  'fill:#0891b2,stroke:#67e8f9,color:#ffffff', // cyan
] as const;

// Larger type + light-on-dark defaults; per-diagram directive so it never leaks
// into chat's mermaid (which renders with the plain 'default' theme).
const INIT =
  "%%{init: {'theme':'base','themeVariables':{'fontSize':'19px','fontFamily':'inherit'," +
  "'primaryColor':'#1e293b','primaryTextColor':'#f8fafc','primaryBorderColor':'#64748b'," +
  "'lineColor':'#cbd5e1','secondaryColor':'#334155','tertiaryColor':'#0f172a'," +
  "'cScale0':'#2563eb','cScale1':'#7c3aed','cScale2':'#059669','cScale3':'#d97706'," +
  "'cScale4':'#db2777','cScale5':'#0891b2'}}}%%";

const CLASS_DEFS = PALETTE.map((p, i) => `classDef c${i} ${p},rx:8,ry:8;`);
const cls = (i: number) => `c${i % PALETTE.length}`;

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
      const lines = [INIT, `flowchart ${dir}`];
      spec.nodes.forEach((n, i) => lines.push(`${nid(i)}["${esc(n.label)}"]`));
      for (const e of spec.edges) {
        const from = order.get(e.from);
        const to = order.get(e.to);
        if (!from || !to) continue;
        lines.push(e.label ? `${from} -->|"${esc(e.label)}"| ${to}` : `${from} --> ${to}`);
      }
      lines.push(...CLASS_DEFS);
      spec.nodes.forEach((_, i) => lines.push(`class ${nid(i)} ${cls(i)}`));
      return lines.join('\n');
    }
    case 'timeline': {
      // mermaid's timeline colors each entry from its cScale palette (set above).
      const lines = [INIT, 'timeline', `  title ${esc(spec.title)}`];
      for (const s of spec.steps) {
        lines.push(`  ${esc(s.label)} : ${esc(s.detail ?? s.label)}`);
      }
      return lines.join('\n');
    }
    case 'cycle': {
      // A closed loop — each node points to the next, the last back to the first.
      const lines = [INIT, 'flowchart LR'];
      spec.nodes.forEach((n, i) => lines.push(`${nid(i)}(["${esc(n.label)}"])`));
      const k = spec.nodes.length;
      for (let i = 0; i < k; i++) lines.push(`${nid(i)} --> ${nid((i + 1) % k)}`);
      lines.push(...CLASS_DEFS);
      spec.nodes.forEach((_, i) => lines.push(`class ${nid(i)} ${cls(i)}`));
      return lines.join('\n');
    }
    case 'pie': {
      // mermaid pie auto-colors its slices.
      const lines = [INIT, `pie showData`, `  title ${esc(spec.title)}`];
      for (const s of spec.slices) lines.push(`  "${esc(s.name)}" : ${Math.max(0, s.value)}`);
      return lines.join('\n');
    }
    case 'sequence': {
      const actors = [...new Set(spec.messages.flatMap((m) => [m.from, m.to]))];
      const id = new Map(actors.map((a, i) => [a, `A${i}`]));
      const lines = [INIT, 'sequenceDiagram'];
      actors.forEach((a) => lines.push(`  participant ${id.get(a)} as ${esc(a)}`));
      for (const m of spec.messages) {
        lines.push(`  ${id.get(m.from)}->>${id.get(m.to)}: ${esc(m.text ?? '')}`);
      }
      return lines.join('\n');
    }
    case 'journey': {
      const lines = [INIT, 'journey', `  title ${esc(spec.title)}`];
      for (const sec of spec.sections) {
        lines.push(`  section ${esc(sec.name)}`);
        for (const st of sec.steps) lines.push(`    ${esc(st.label)}: ${st.score}: Me`);
      }
      return lines.join('\n');
    }
    case 'quadrant': {
      const lines = [
        INIT,
        'quadrantChart',
        `  title ${esc(spec.title)}`,
        `  x-axis ${esc(spec.xAxis[0])} --> ${esc(spec.xAxis[1])}`,
        `  y-axis ${esc(spec.yAxis[0])} --> ${esc(spec.yAxis[1])}`,
      ];
      for (const p of spec.points) lines.push(`  ${esc(p.label)}: [${p.x.toFixed(2)}, ${p.y.toFixed(2)}]`);
      return lines.join('\n');
    }
    case 'mindmap': {
      // A radial mind map — central topic → branches → leaves. mermaid's
      // mindmap auto-colors each branch, so this reads like the NotebookLM /
      // Excalidraw references. Hierarchy is by indentation; no init directive
      // (mindmap doesn't take the flowchart theme vars cleanly).
      const lines = ['mindmap', `  root((${esc(spec.title)}))`];
      for (const b of spec.branches) {
        lines.push(`    ${esc(b.label)}`);
        for (const c of b.children) lines.push(`      ${esc(c)}`);
      }
      return lines.join('\n');
    }
    case 'compare': {
      // Each item is ONE colored card — its name on top, then its points as
      // bullet lines. Invisible links chain the cards left-to-right so they read
      // as aligned side-by-side columns. The old version used one box per point
      // in disconnected `flowchart TD` subgraphs, which mermaid stacked
      // vertically into a scattered eyesore. Points are escaped individually
      // then joined with <br/> (a raw <br/> would be stripped by esc); mermaid
      // renders <br/> as a line break, so each card is a tidy titled list.
      const lines = [INIT, 'flowchart LR'];
      spec.items.forEach((item, i) => {
        const parts = [esc(item.name), ...item.points.map((p) => `• ${esc(p)}`)];
        lines.push(`${nid(i)}["${parts.join('<br/>')}"]`);
      });
      for (let i = 1; i < spec.items.length; i++) lines.push(`${nid(i - 1)} ~~~ ${nid(i)}`);
      lines.push(...CLASS_DEFS);
      spec.items.forEach((_, i) => lines.push(`class ${nid(i)} ${cls(i)}`));
      return lines.join('\n');
    }
  }
}
