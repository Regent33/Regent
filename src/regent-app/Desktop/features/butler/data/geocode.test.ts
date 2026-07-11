import { expect, test } from 'bun:test';
import { hasPlaceCandidate, placeCandidates } from './geocode';

test('pulls the place out of explicit location asks', () => {
  expect(placeCandidates('where is China')).toContain('China');
  expect(placeCandidates('show me Tokyo on the map')).toContain('Tokyo');
  expect(placeCandidates('take me to Paris please')).toContain('Paris');
  expect(placeCandidates("what's the weather in New York")).toContain('New York');
  expect(placeCandidates('navigate to the Eiffel Tower')).toContain('Eiffel Tower');
});

test('a bare mention in an explanation is NOT a candidate (no globe hijack)', () => {
  // The whole point of the fix: explaining a topic that names a place must not
  // open the map. Only an explicit location cue does.
  expect(placeCandidates('Pampanga is famous for sisig')).toEqual([]);
  expect(placeCandidates('the history of Rome spans centuries')).toEqual([]);
  expect(placeCandidates('tell me about the Eiffel Tower')).toEqual([]);
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
