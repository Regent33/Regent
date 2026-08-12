// Every diagram type the app can draw must also draw in CHAT, under every
// fence tag a model realistically reaches for.
//
// The two chat paths recognise spec blocks differently: `extractPresentSpec`
// (whole-message, used by MessageRow) accepts ANY fence tag, while
// `specFromCode` (per-block, used by Markdown) had a three-entry allow-list.
// So a block fenced ```timeline drew when it was the reply's only diagram and
// rendered as raw JSON when it was the second one, or sat inside prose. Same
// spec, same message, two different outcomes.
import { describe, expect, test } from 'bun:test';
import { specFromCode } from '@/shared/ui/markdown/SpecDiagram';
import { specToMermaid } from '@/shared/diagram/diagramMermaid';
import type { PresentSpec } from '@/shared/diagram/presentSpec';

/** One minimal VALID spec per type — the smallest thing `validate` accepts,
 * so the fixtures fail if a type's floor ever moves. */
const SPECS: Record<PresentSpec['type'], string> = {
  flow: '{"type":"flow","title":"F","nodes":[{"id":"a","label":"A"},{"id":"b","label":"B"}],"edges":[{"from":"a","to":"b"}]}',
  concept:
    '{"type":"concept","title":"C","nodes":[{"id":"a","label":"A"},{"id":"b","label":"B"}],"edges":[{"from":"a","to":"b"}]}',
  cycle: '{"type":"cycle","title":"Cy","nodes":[{"id":"a","label":"A"},{"id":"b","label":"B"}]}',
  timeline: '{"type":"timeline","title":"T","steps":[{"label":"1914"},{"label":"1918"}]}',
  compare:
    '{"type":"compare","title":"Cmp","items":[{"name":"X","points":["p"]},{"name":"Y","points":["q"]}]}',
  mindmap: '{"type":"mindmap","title":"M","branches":[{"label":"B","children":["c"]}]}',
  pie: '{"type":"pie","title":"P","slices":[{"name":"A","value":60},{"name":"B","value":40}]}',
  sequence: '{"type":"sequence","title":"S","messages":[{"from":"A","to":"B","text":"hi"}]}',
  journey:
    '{"type":"journey","title":"J","sections":[{"name":"N","steps":[{"label":"L","score":3}]}]}',
  quadrant:
    '{"type":"quadrant","title":"Q","xAxis":["lo","hi"],"yAxis":["lo","hi"],"points":[{"label":"P","x":0.5,"y":0.5}]}',
};

const TYPES = Object.keys(SPECS) as Array<PresentSpec['type']>;

describe('chat renders every diagram type the app supports', () => {
  test('covers all ten types — a new type must be added here too', () => {
    expect(TYPES).toHaveLength(10);
  });

  for (const type of TYPES) {
    const body = SPECS[type];

    test(`${type}: accepted under json/present/untagged and its own type name`, () => {
      // The tags the prompt asks for, plus the one a model invents from the
      // spec's own "type" field.
      for (const tag of ['json', 'present', '', type, type.toUpperCase(), ` ${type} `]) {
        const spec = specFromCode(tag, body);
        // A null here means this tag rejected a valid spec — chat would have
        // shown the user raw JSON where the app draws a diagram.
        expect([tag, spec === null]).toEqual([tag, false]);
        expect(spec?.type).toBe(type === 'concept' ? 'concept' : type);
      }
    });

    test(`${type}: converts to non-empty mermaid`, () => {
      const spec = specFromCode('json', body);
      expect(spec).not.toBeNull();
      // A type the converter forgot would fall through its switch and yield
      // nothing to render — the failure the ten cases exist to prevent.
      expect(specToMermaid(spec as PresentSpec).trim().length).toBeGreaterThan(0);
    });
  }

  test('leaves real code blocks alone', () => {
    // A language tag we never treat as a spec.
    expect(specFromCode('python', SPECS.flow)).toBeNull();
    // JSON that is not a spec stays a code block, whatever its tag.
    expect(specFromCode('json', '{"type":"module","main":"index.js"}')).toBeNull();
    expect(specFromCode('timeline', '{"not":"a spec"}')).toBeNull();
    // Not an object at all.
    expect(specFromCode('json', '[1,2,3]')).toBeNull();
    expect(specFromCode('', 'plain prose')).toBeNull();
  });
});
