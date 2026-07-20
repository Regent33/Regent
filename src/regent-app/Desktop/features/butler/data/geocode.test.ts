import { expect, test } from 'bun:test';
import { hasPlaceCandidate, placeCandidates } from './geocode';

test('pulls the place out of explicit location asks', () => {
  expect(placeCandidates('where is China')).toContain('China');
  expect(placeCandidates('show me Tokyo on the map')).toContain('Tokyo');
  expect(placeCandidates('take me to Paris please')).toContain('Paris');
  expect(placeCandidates("what's the weather in New York")).toContain('New York');
  expect(placeCandidates('navigate to the Eiffel Tower')).toContain('Eiffel Tower');
});

test('"where the X is [in Y]" opens the map (subject sits between where and is)', () => {
  // Field bug: STT gave "where the Tesla factory is on China" — the adjacent
  // "where is" cue missed it, the map never opened, and Butler hit the web.
  expect(hasPlaceCandidate('Can you show me where the Tesla factory is on China?')).toBe(true);
  const c = placeCandidates('where the Tesla factory is in China');
  // Comma, not a bare space: "Tesla factory china" reads as loose keywords and
  // let the US-based company outrank the Shanghai gigafactory it meant.
  expect(c).toContain('Tesla factory, China'); // sharper combined query, tried first
  expect(c).toContain('Tesla factory');
});

test('a doubled "where is X is in Y" (STT repeating "is") does not leak "is" into the subject', () => {
  // Field bug: "where is tesla factory is on china" resolved to the US — the
  // subject capture greedily swallowed the FIRST "is" ("is tesla factory"),
  // and the space-joined query let a US Tesla result outrank Shanghai.
  const c = placeCandidates('where is tesla factory is on china');
  expect(c).toContain('tesla factory, china');
  expect(c).not.toContain('is tesla factory, china');
  expect(c).not.toContain('is tesla factory');
});

test('an explanation ask never opens the map — even "show me how…"', () => {
  // The exact bug from the field: "show me how X works" must NOT geocode "how".
  expect(placeCandidates('can you show me how photosynthesis works')).toEqual([]);
  expect(placeCandidates('show me how for the synthesis works')).toEqual([]);
  expect(placeCandidates('explain what the water cycle is')).toEqual([]);
  expect(placeCandidates('Pampanga is famous for sisig')).toEqual([]);
  expect(placeCandidates('the history of Rome spans centuries')).toEqual([]);
});

test('filler and non-place chatter yields no candidate', () => {
  expect(placeCandidates('No, no, no. Got it, dropping it.')).toEqual([]);
  expect(hasPlaceCandidate('okay sure, right now')).toBe(false);
  expect(placeCandidates("where's my file")).toEqual([]);
});

test('bare "where" without its verb is conversation, not a map ask', () => {
  expect(placeCandidates('where do we start')).toEqual([]);
  expect(placeCandidates("that's where things got interesting")).toEqual([]);
  expect(placeCandidates('where were we yesterday')).toEqual([]);
});

test('trailing punctuation is trimmed off candidates', () => {
  expect(placeCandidates('where is Berlin?')).toContain('Berlin');
});

test('leading article is stripped (the Eiffel Tower → Eiffel Tower, not a US replica)', () => {
  const c = placeCandidates('where is the Eiffel Tower');
  expect(c).toContain('Eiffel Tower');
  expect(c).not.toContain('the Eiffel Tower');
});

test('a follow-up names a new place only while the map already holds the stage', () => {
  // The live bug: with the globe already up, asking for the NEXT place carried
  // no explicit cue, so nothing resolved and the map stayed on the first one.
  for (const said of ['what about Tokyo', 'now Japan', 'and Rome?', 'go to Madrid']) {
    expect(hasPlaceCandidate(said, true)).toBe(true);
    expect(hasPlaceCandidate(said, false)).toBe(false); // far too loose in open conversation
  }
  expect(placeCandidates('what about Tokyo', true)).toContain('Tokyo');
});

test('a follow-up cue never steals the stage from a non-place request', () => {
  // These reach the SYNC check, which has no geocode backstop and decides
  // whether a place ask owns the stage — a diagram question asked while the
  // map is up must not be mistaken for one.
  for (const said of [
    'and how does it work',
    'now explain the Krebs cycle',
    'show me how DNS resolves a name',
    'what about that, can you compare them',
  ]) {
    expect(hasPlaceCandidate(said, true)).toBe(false);
  }
});

test('a short topic ask is never a place ask, even with the map open', () => {
  // Regression: the loose follow-up matcher also gates DIAGRAMS (a place ask
  // owns the stage). After a run of place questions it accepted any short
  // noun, so "show me photosynthesis" suppressed the diagram — while the
  // geocode correctly found no such place, so the map did not move either.
  // Nothing appeared. Stage ownership must therefore stay on the STRICT form.
  for (const said of ['show me photosynthesis', 'now the water cycle', 'and mitosis', 'what about gravity']) {
    expect(hasPlaceCandidate(said)).toBe(false); // strict — decides the stage
  }
});
