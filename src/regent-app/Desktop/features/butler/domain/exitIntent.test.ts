import { describe, expect, test } from 'bun:test';
import { isExitButlerCommand } from './exitIntent';

describe('spoken "exit butler mode"', () => {
  test('closes on an explicit command, however it is phrased', () => {
    for (const said of [
      'exit butler mode',
      'Exit Butler Mode.',
      'close butler',
      'quit butler mode please',
      'please exit butler mode',
      'can you close butler mode',
      "let's exit butler mode",
      'hey Regent, exit butler mode',
      'end the call',
      'get out of butler mode',
      'turn off butler mode',
      'exit',
      'close.',
      // Live repro: trailing filler after a bare command. Regent answered
      // "I can't exit myself" instead of the surface simply closing.
      'Please exit now.',
      'exit now',
      'can you exit now',
      'okay regent, close now please',
    ]) {
      expect(isExitButlerCommand(said)).toBe(true);
    }
  });

  test('never closes on a sentence that merely mentions leaving', () => {
    // A false positive destroys a live conversation, so these must all stay.
    for (const said of [
      'how do I exit vim',
      'close the map',
      'what does it mean to exit a process',
      'stop, that is not what I meant',
      'can you explain how to close a business',
      'end of the road, what happens next',
      'quit smoking is hard, why',
      'exit strategies for startups',
      'tell me about the butler in that novel',
      'what is butler mode',
      'how does butler mode work',
    ]) {
      expect(isExitButlerCommand(said)).toBe(false);
    }
  });

  test('empty or blank input is never a command', () => {
    expect(isExitButlerCommand('')).toBe(false);
    expect(isExitButlerCommand('   ')).toBe(false);
  });
});
