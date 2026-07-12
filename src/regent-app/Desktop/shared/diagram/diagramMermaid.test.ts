// Each spec type produces parseable-looking mermaid source. String assertions
// only — no mermaid import, so this stays a fast pure-logic test.
import { describe, expect, test } from 'bun:test';
import { specToMermaid } from './diagramMermaid';
import type { PresentSpec } from '@/shared/diagram/presentSpec';

describe('specToMermaid', () => {
  test('flow → flowchart TD with nodes and a labelled edge', () => {
    const spec: PresentSpec = {
      type: 'flow',
      title: 'x',
      nodes: [
        { id: 'a', label: 'Start' },
        { id: 'b', label: 'End' },
      ],
      edges: [{ from: 'a', to: 'b', label: 'go' }],
    };
    const src = specToMermaid(spec);
    expect(src).toContain('flowchart TD');
    expect(src).toContain('n0["Start"]');
    expect(src).toContain('n0 -->|"go"| n1');
  });

  test('concept → flowchart LR', () => {
    const src = specToMermaid({
      type: 'concept',
      title: 'x',
      nodes: [{ id: 'a', label: 'A' }],
      edges: [],
    });
    expect(src).toContain('flowchart LR');
    expect(src).toContain('n0["A"]');
  });

  test('timeline → timeline with title and per-step entries', () => {
    const src = specToMermaid({
      type: 'timeline',
      title: 'History',
      steps: [
        { label: '2001', detail: 'Founded' },
        { label: '2010' },
      ],
    });
    expect(src).toContain('%%{init'); // styling directive prepended for color/size
    expect(src).toContain('\ntimeline');
    expect(src).toContain('title History');
    expect(src).toContain('2001 : Founded');
    expect(src).toContain('2010 : 2010');
  });

  test('flow nodes get color classes (never all-gray)', () => {
    const src = specToMermaid({
      type: 'flow',
      title: 'x',
      nodes: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }],
      edges: [],
    });
    expect(src).toContain('classDef c0');
    expect(src).toContain('class n0 c0');
    expect(src).toContain('class n1 c1');
  });

  test('compare → flowchart with one subgraph per item', () => {
    const src = specToMermaid({
      type: 'compare',
      title: 'x',
      items: [
        { name: 'Cats', points: ['aloof'] },
        { name: 'Dogs', points: ['loyal', 'loud'] },
      ],
    });
    expect(src).toContain('subgraph g0["Cats"]');
    expect(src).toContain('subgraph g1["Dogs"]');
    expect(src).toContain('g1p1["loud"]');
    expect((src.match(/end/g) ?? []).length).toBe(2);
  });

  test('mindmap → radial root with indented branches and children', () => {
    const src = specToMermaid({
      type: 'mindmap',
      title: 'Photosynthesis',
      branches: [
        { label: 'Inputs', children: ['Sunlight', 'CO2', 'Water'] },
        { label: 'Outputs', children: ['Glucose', 'Oxygen'] },
      ],
    });
    expect(src.startsWith('mindmap')).toBe(true);
    expect(src).toContain('root((Photosynthesis))');
    expect(src).toContain('    Inputs');
    expect(src).toContain('      Sunlight');
    expect(src).toContain('    Outputs');
  });

  test('cycle → closed loop, last node points back to the first', () => {
    const src = specToMermaid({
      type: 'cycle',
      title: 'x',
      nodes: [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }, { id: 'c', label: 'C' }],
    });
    expect(src).toContain('flowchart LR');
    expect(src).toContain('n2 --> n0'); // wraps around
  });

  test('pie / sequence / journey / quadrant emit their diagram headers', () => {
    expect(specToMermaid({ type: 'pie', title: 'x', slices: [{ name: 'A', value: 30 }, { name: 'B', value: 70 }] })).toContain('pie showData');
    expect(specToMermaid({ type: 'sequence', title: 'x', messages: [{ from: 'A', to: 'B', text: 'hi' }] })).toContain('sequenceDiagram');
    expect(specToMermaid({ type: 'journey', title: 'x', sections: [{ name: 'S', steps: [{ label: 'T', score: 4 }] }] })).toContain('journey');
    expect(specToMermaid({ type: 'quadrant', title: 'x', xAxis: ['Lo', 'Hi'], yAxis: ['Lo', 'Hi'], points: [{ label: 'P', x: 0.3, y: 0.7 }] })).toContain('quadrantChart');
  });

  test('quotes and backticks in labels are sanitized away', () => {
    const src = specToMermaid({
      type: 'flow',
      title: 'x',
      nodes: [{ id: 'a', label: 'say "hi" `now`' }],
      edges: [],
    });
    expect(src).not.toContain('"hi"');
    expect(src).not.toContain('`');
    expect(src).toContain('n0["say hi now"]');
  });
});
