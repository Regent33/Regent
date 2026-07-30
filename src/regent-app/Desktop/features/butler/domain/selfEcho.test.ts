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
    expect(isSelfEcho('it takes acetyl CoA and releases', '')).toBe(false);
  });

  // BEHAVIOUR CHANGE (2026-07-31), not a corrected assertion. This case used to
  // assert false: anything under four tokens skipped the check outright. That
  // is the hole the owner hit — once the endpoint window widened, most of what
  // the mic caught of Regent's own voice was a two- or three-word fragment, and
  // every one of them was promoted as a real interruption. A short fragment now
  // has to match in FULL, which is stricter than the four-token rule, not
  // looser.
  test('a short verbatim fragment of the reply is still echo', () => {
    expect(isSelfEcho('the mitochondria', REPLY)).toBe(true);
    expect(isSelfEcho('carbon dioxide', REPLY)).toBe(true);
    // Partial overlap is not a full match, so it still gets through.
    expect(isSelfEcho('mitochondria please', REPLY)).toBe(false);
  });

  // The cost of the above, held to one word: a single word is what a real barge
  // sounds like, so it is never vetoed even when the reply contains it.
  test('a one-word barge always interrupts, even a word Regent just said', () => {
    expect(isSelfEcho('stop', REPLY)).toBe(false);
    expect(isSelfEcho('mitochondria', REPLY)).toBe(false);
  });
});
