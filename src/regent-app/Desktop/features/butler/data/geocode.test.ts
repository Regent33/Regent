import { expect, test } from 'bun:test';
import { hasPlaceCandidate, placeCandidates } from './geocode';

test('pulls the place out of explicit location asks', () => {
  expect(placeCandidates('where is China')).toContain('China');
  expect(placeCandidates('show me Tokyo on the map')).toContain('Tokyo');
  expect(placeCandidates('take me to Paris please')).toContain('Paris');
  expect(placeCandidates("what's the weather in New York")).toContain('New York');
  expect(placeCandidates('navigate to the Eiffel Tower')).toContain('Eiffel Tower');
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

test('trailing punctuation is trimmed off candidates', () => {
  expect(placeCandidates('where is Berlin?')).toContain('Berlin');
});

test('leading article is stripped (the Eiffel Tower → Eiffel Tower, not a US replica)', () => {
  const c = placeCandidates('where is the Eiffel Tower');
  expect(c).toContain('Eiffel Tower');
  expect(c).not.toContain('the Eiffel Tower');
});
