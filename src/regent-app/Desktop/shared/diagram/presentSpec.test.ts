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

  test('strips a printed tool call from the archived prose (not just the caption)', () => {
    const reply = 'Ok. {"action":"screenshot"} Here it is. ```present\n{"type":"flow","title":"T","nodes":["A"],"edges":[]}\n```';
    const { spec, text } = extractPresentSpec(reply);
    expect(spec?.type).toBe('flow');
    expect(text).toBe('Ok. Here it is.');
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

  test('cuts a bare tool-call JSON a weak model printed as text', () => {
    // {"action":"screenshot",…} must not flash in the spoken caption.
    expect(
      stripPresentTail('Sure. {"action":"screenshot","question":"What is on screen?"}'),
    ).toBe('Sure.');
    expect(stripPresentTail('{"action":"screenshot"}')).toBe('');
  });

  test('cuts a tool call printed BEFORE the diagram fence (not just trailing)', () => {
    // The leak: the fence cut fires first and keeps everything before it, so a
    // tool call printed ahead of the diagram survived in the caption.
    const reply = 'Let me look. {"action":"screenshot","question":"?"} Here is the flow. ```present\n{"type":"flow","title":"T"}\n```';
    expect(stripPresentTail(reply)).toBe('Let me look. Here is the flow.');
  });

  test('an UNFENCED leading spec still fires, with the speech after it kept', () => {
    // Weak voice models routinely drop the fence. The trailing-object scan can
    // never match these (prose follows the object), so the diagram was lost.
    const reply =
      '{"type":"pie","title":"Budget","slices":[{"name":"Rent","value":60},' +
      '{"name":"Food","value":40}]}\nHere is how the budget splits.';
    const { spec, text } = extractPresentSpec(reply);
    expect(spec?.type).toBe('pie');
    expect(text).toBe('Here is how the budget splits.');
  });

  test('an unfenced leading spec leaves the caption its speech, not blank', () => {
    const live =
      '{"type":"cycle","title":"Loop","nodes":[{"id":"a","label":"A"}]}\nAnd round it goes.';
    expect(stripPresentTail(live)).toBe('And round it goes.');
  });

  test('a half-streamed unfenced spec still blanks the caption (no JSON flash)', () => {
    expect(stripPresentTail('{"type":"pie","title":"Bud')).toBe('');
  });
});
