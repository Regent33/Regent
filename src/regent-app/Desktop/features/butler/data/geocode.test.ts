import { expect, test } from 'bun:test';
import { hasPlaceCandidate, placeCandidates } from './geocode';

test('pulls the place out of common location asks', () => {
  expect(placeCandidates('where is China')).toContain('China');
  expect(placeCandidates('show me Tokyo on the map')).toContain('Tokyo');
  expect(placeCandidates('take me to Paris please')).toContain('Paris');
  expect(placeCandidates("what's the weather in New York")).toContain('New York');
  expect(placeCandidates('tell me about the Eiffel Tower')).toContain('Eiffel Tower');
});

test('a bare mention still surfaces as a candidate (the geocoder gates it)', () => {
  expect(placeCandidates('Pampanga is famous for sisig')).toContain('Pampanga');
});

test('filler and non-place chatter yields no candidate', () => {
  expect(placeCandidates('No, no, no. Got it, dropping it.')).toEqual([]);
  expect(hasPlaceCandidate('okay sure, right now')).toBe(false);
});

test('trailing punctuation is trimmed off candidates', () => {
  expect(placeCandidates('where is Berlin?')).toContain('Berlin');
});

test('leading article is stripped (the Eiffel Tower → Eiffel Tower, not a US replica)', () => {
  const c = placeCandidates('where is the Eiffel Tower');
  expect(c).toContain('Eiffel Tower');
  expect(c).not.toContain('the Eiffel Tower');
});
