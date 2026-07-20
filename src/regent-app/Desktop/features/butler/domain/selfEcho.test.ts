import { describe, expect, test } from 'bun:test';
import { isSelfEcho } from './selfEcho';

const REPLY =
  'The Krebs cycle runs in the mitochondria. It takes acetyl CoA and releases carbon dioxide.';

describe('self-echo detection', () => {
  test("Regent's own sentence coming back through the mic is caught", () => {
    expect(isSelfEcho('it takes acetyl CoA and releases carbon dioxide', REPLY)).toBe(true);
  });

  test('ASR punctuation/casing drift does not hide the echo', () => {
    expect(isSelfEcho('The Krebs Cycle runs, in the mitochondria!', REPLY)).toBe(true);
  });

  test('a real interruption is never mistaken for echo', () => {
    for (const barge of ['wait', 'no, stop', 'hold on a second', 'can you explain that differently']) {
      expect(isSelfEcho(barge, REPLY)).toBe(false);
    }
  });

  test('incidental shared words are not enough — only a verbatim run counts', () => {
    // Shares "the", "cycle", "carbon" with the reply but in the caller's own
    // sentence: a real question about the topic must still interrupt.
    expect(isSelfEcho('so is the carbon released in that cycle wasted', REPLY)).toBe(false);
  });

  test('too little to judge fails toward letting the caller through', () => {
    expect(isSelfEcho('', REPLY)).toBe(false);
    expect(isSelfEcho('the mitochondria', REPLY)).toBe(false); // 2 tokens
    expect(isSelfEcho('it takes acetyl CoA and releases', '')).toBe(false);
  });
});
