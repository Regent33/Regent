// The reported problem: everything the user said and everything Regent said
// bled onto the diagram — whole spoken sentences as timeline steps, "Sure! Let
// me walk you through this." as a mind-map branch. Only the relevant points
// belong there, and a label is a phrase, not a sentence.
import { describe, expect, test } from 'bun:test';
import { explanationPoints } from './explanationPoints';

describe('explanationPoints', () => {
  test("the model's own enumeration wins — that IS the relevant set", () => {
    // The old code stripped these markers and flattened the reply before it
    // ever looked for structure, then chopped the prose into sentences.
    const reply = [
      'Sure! Let me walk you through the water cycle.',
      '',
      '- Evaporation',
      '- Condensation',
      '- Precipitation',
      '- Collection',
      '',
      'Hope that helps — let me know if you want more detail.',
    ].join('\n');
    expect(explanationPoints(reply)).toEqual([
      'Evaporation',
      'Condensation',
      'Precipitation',
      'Collection',
    ]);
  });

  test('numbered lists count as enumeration too', () => {
    const reply = '1. Mix the dry\n2) Add the wet\n3. Bake at 180C';
    expect(explanationPoints(reply)).toEqual(['Mix the dry', 'Add the wet', 'Bake at 180C']);
  });

  test('conversational scaffolding never becomes a node', () => {
    const reply =
      'Of course. Photosynthesis converts light into sugar. Chlorophyll absorbs the light. Hope this helps.';
    const points = explanationPoints(reply);
    expect(points).toEqual([
      'Photosynthesis converts light into sugar',
      'Chlorophyll absorbs the light',
    ]);
  });

  test('a label is the leading clause, not the whole sentence', () => {
    // The elaboration after a dash or a "which" clause is what turns a label
    // into a paragraph on the stage.
    const reply =
      'Mitosis splits one cell into two — a process that takes about an hour in most human tissues and is tightly regulated.';
    expect(explanationPoints(reply)).toEqual(['Mitosis splits one cell into two']);
  });

  test('an over-long label is cut on a word boundary, never mid-word', () => {
    const reply = 'alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima.';
    const [label] = explanationPoints(reply);
    expect(label.endsWith('…')).toBe(true);
    expect(label.length).toBeLessThanOrEqual(49);
    // The kept text must be a whole-word prefix of the reply: the character
    // that follows it in the original is a space, so no word was sliced.
    const kept = label.slice(0, -1);
    expect(reply.startsWith(kept)).toBe(true);
    expect(reply[kept.length]).toBe(' ');
  });

  test('a single stray dash in prose is not an enumeration', () => {
    // One bullet means the model wrote a sentence with a dash, not a list.
    const reply = 'The engine stalls when cold. - usually below five degrees.';
    expect(explanationPoints(reply).length).toBeGreaterThan(1);
  });

  test('a purely conversational turn yields nothing, so it gets no diagram', () => {
    expect(explanationPoints('Sure! Of course. Hope that helps.')).toEqual([]);
    expect(explanationPoints('')).toEqual([]);
  });

  test('code fences and links never reach the stage', () => {
    const reply = 'Install it first.\n```\nnpm i regent\n```\nSee https://example.com/docs for more.';
    const points = explanationPoints(reply);
    expect(points.join(' ')).not.toContain('npm i regent');
    expect(points.join(' ')).not.toContain('example.com');
  });

  test('at most eight nodes — past that a diagram is a list with extra steps', () => {
    const reply = Array.from({ length: 20 }, (_, i) => `- point ${i}`).join('\n');
    expect(explanationPoints(reply)).toHaveLength(8);
  });
});
