// Trust-boundary tests for the ```present diagram spec parser: a valid spec
// parses, off-shape specs are rejected to null, and the streaming-caption
// stripper removes partial/complete blocks.
import { describe, expect, test } from 'bun:test';
import { extractPresentSpec, stripPresentTail } from './presentSpec';

function fenced(json: unknown): string {
  return `Here is how it works. \`\`\`present\n${JSON.stringify(json)}\n\`\`\``;
}

describe('extractPresentSpec', () => {
  test('parses a valid flow spec and strips the block from the text', () => {
    const reply = fenced({
      type: 'flow',
      title: 'Request lifecycle',
      nodes: [
        { id: 'a', label: 'Client' },
        { id: 'b', label: 'Server' },
      ],
      edges: [{ from: 'a', to: 'b', label: 'request' }],
    });
    const { spec, text } = extractPresentSpec(reply);
    expect(spec?.type).toBe('flow');
    expect(text).toBe('Here is how it works.');
    expect(spec && spec.type === 'flow' && spec.nodes.length).toBe(2);
  });

  test('rejects an unknown type', () => {
    expect(extractPresentSpec(fenced({ type: 'pie', title: 'x', nodes: [], edges: [] })).spec).toBeNull();
  });

  test('rejects an oversized spec (too many nodes)', () => {
    const nodes = Array.from({ length: 17 }, (_, i) => ({ id: `n${i}`, label: `L${i}` }));
    expect(extractPresentSpec(fenced({ type: 'flow', title: 'big', nodes, edges: [] })).spec).toBeNull();
  });

  test('drops a dangling edge but keeps the diagram (robust to loose specs)', () => {
    const reply = fenced({
      type: 'flow',
      title: 'x',
      nodes: [{ id: 'a', label: 'A' }],
      edges: [{ from: 'a', to: 'ghost' }],
    });
    const spec = extractPresentSpec(reply).spec;
    expect(spec?.type).toBe('flow');
    expect(spec && spec.type === 'flow' && spec.edges.length).toBe(0);
  });

  test('accepts a ```json fence and string-array nodes (model-shape leniency)', () => {
    const reply = 'Sure. ```json\n{"type":"flow","title":"T","nodes":["Sun","Plant"],"edges":[{"from":"Sun","to":"Plant"}]}\n```';
    const spec = extractPresentSpec(reply).spec;
    expect(spec?.type).toBe('flow');
    expect(spec && spec.type === 'flow' && spec.nodes.map((n) => n.label)).toEqual(['Sun', 'Plant']);
  });

  test('accepts a bare trailing JSON object (no fence)', () => {
    const reply = 'Here you go. {"type":"timeline","title":"T","steps":["First","Then","Last"]}';
    const spec = extractPresentSpec(reply).spec;
    expect(spec?.type).toBe('timeline');
    expect(spec && spec.type === 'timeline' && spec.steps.length).toBe(3);
  });

  test('accepts a mindmap spec (branches + children)', () => {
    const reply = fenced({
      type: 'mindmap',
      title: 'Topic',
      branches: [
        { label: 'A', children: ['a1', 'a2'] },
        { label: 'B', children: ['b1'] },
      ],
    });
    const spec = extractPresentSpec(reply).spec;
    expect(spec?.type).toBe('mindmap');
    expect(spec && spec.type === 'mindmap' && spec.branches.length).toBe(2);
    expect(spec && spec.type === 'mindmap' && spec.branches[0].children).toEqual(['a1', 'a2']);
  });

  test('tolerates a trailing extra } inside the fence (real model glitch)', () => {
    // Observed live: a valid timeline followed by a duplicate closing brace.
    // strict JSON.parse rejected the whole block and no diagram rendered.
    const reply = 'Here you go. ```json\n{"type":"timeline","title":"T","steps":["A","B"]}}\n```';
    const spec = extractPresentSpec(reply).spec;
    expect(spec?.type).toBe('timeline');
    expect(spec && spec.type === 'timeline' && spec.steps.length).toBe(2);
  });

  test('tolerates prose accidentally left after the object inside the fence', () => {
    const reply = '```json\n{"type":"flow","title":"T","nodes":["A"],"edges":[]}\nthat is the flow.\n```\nDone.';
    expect(extractPresentSpec(reply).spec?.type).toBe('flow');
  });

  test('no block → spec null, text unchanged', () => {
    const { spec, text } = extractPresentSpec('Just talking, no diagram.');
    expect(spec).toBeNull();
    expect(text).toBe('Just talking, no diagram.');
  });
});

describe('stripPresentTail', () => {
  test('cuts a complete block', () => {
    expect(stripPresentTail('Prose here. ```present\n{"type":"flow"}\n```')).toBe('Prose here.');
  });

  test('cuts a half-streamed block (partial label + JSON)', () => {
    expect(stripPresentTail('Prose here. ```present\n{"type":"fl')).toBe('Prose here.');
    expect(stripPresentTail('Prose here. ```pres')).toBe('Prose here.');
    expect(stripPresentTail('Prose here. ```')).toBe('Prose here.');
  });

  test('leaves an unrelated trailing fence alone', () => {
    expect(stripPresentTail('run ```bash')).toBe('run ```bash');
  });

  test('the spec now LEADS: a complete leading block is dropped, prose after it shows', () => {
    const reply = '```json\n{"type":"flow","title":"T"}\n```\nHere is how it works.';
    expect(stripPresentTail(reply)).toBe('Here is how it works.');
  });

  test('while a leading block is still streaming, the caption is blank (no JSON flash)', () => {
    expect(stripPresentTail('```json\n{"type":"fl')).toBe('');
  });

  test('a leading non-spec code block is NOT treated as a spec', () => {
    const reply = '```bash\necho hi\n```\nrest';
    expect(stripPresentTail(reply)).toBe(reply); // no "type" → untouched
  });
});
