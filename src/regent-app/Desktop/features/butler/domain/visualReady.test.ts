import { describe, expect, test } from 'bun:test';
import { extractPresentSpec } from '@/shared/diagram/presentSpec';
import {
  createVisualReadyGate,
  expectsVisualExplanation,
  fallbackPresentSpec,
  isConversationalTurn,
} from './visualReady';

describe('visual-ready gate', () => {
  test('holds audio until the visual releases it', async () => {
    const gate = createVisualReadyGate(1_000);
    let ready = false;
    const waiting = gate.wait().then(() => {
      ready = true;
    });
    await Promise.resolve();
    expect(ready).toBe(false);
    gate.release();
    await waiting;
    expect(ready).toBe(true);
  });

  test('fails open after its deadlock timeout', async () => {
    const gate = createVisualReadyGate(5);
    await gate.wait();
  });
});

describe('visual explanation intent', () => {
  test('recognizes explanations and comparisons', () => {
    expect(expectsVisualExplanation('Explain how automatic fallback works')).toBe(true);
    expect(expectsVisualExplanation('Compare Ollama versus NVIDIA')).toBe(true);
  });

  test('does not delay an ordinary command', () => {
    expect(expectsVisualExplanation('Play this song')).toBe(false);
  });

  test('a greeting is small talk, not an explainer (no diagram for "how are u")', () => {
    for (const greeting of ['how are u', 'How are you', 'how is it going', "how's it going", 'how you doing', 'hey there', 'good morning']) {
      expect(expectsVisualExplanation(greeting)).toBe(false);
    }
    // But "how are X related" is still a real ask — the greeting guard is object-scoped.
    expect(expectsVisualExplanation('how are neurons and synapses related')).toBe(true);
    expect(expectsVisualExplanation('how does an engine work')).toBe(true);
  });

  test('recognizes every supported explainer family', () => {
    for (const request of [
      'Give me an overview of transformers',
      'Show the history of the web',
      'Explain the cycle of water',
      'Show the interaction sequence',
      'Map the customer journey',
      'Show the percentage distribution',
      'Make an effort impact matrix',
      'Show the relationship between these concepts',
    ]) {
      expect(expectsVisualExplanation(request)).toBe(true);
    }
  });
});

describe('deterministic explainer fallback', () => {
  test('builds valid specs for all ten diagram types', () => {
    const cases: Array<[string, string]> = [
      ['Explain how an engine works', 'Intake begins. Compression follows. Power is produced. Exhaust exits.'],
      ['Show the relationship concept map', 'Users request work. Regent selects tools. Tools return results.'],
      ['Explain the water cycle', 'Water evaporates. Clouds form. Rain falls. Water returns.'],
      ['Show the history timeline', 'The project began. The first release shipped. Adoption grew.'],
      ['Compare Ollama versus NVIDIA', 'Ollama runs locally. NVIDIA offers hosted inference. Both need setup.'],
      ['Give me an overview breakdown', 'There is a core. It has adapters. The UI uses the core.'],
      ['Show the percentage distribution', 'Engineering is largest. Design follows. Operations is smaller.'],
      ['Explain the interaction sequence', 'A client sends a request. The server replies. The client renders it.'],
      ['Map the customer journey', 'A user discovers it. They try it. They adopt it.'],
      ['Make an effort impact matrix', 'Quick wins are first. Larger bets follow. Maintenance continues.'],
    ];
    const expected = ['flow', 'concept', 'cycle', 'timeline', 'compare', 'mindmap', 'pie', 'sequence', 'journey', 'quadrant'];

    cases.forEach(([heard, reply], index) => {
      const spec = fallbackPresentSpec(heard, reply);
      expect(spec?.type).toBe(expected[index]);
      expect(extractPresentSpec(JSON.stringify(spec)).spec?.type).toBe(expected[index]);
    });
  });
});

// Field report, with screenshots: closing a call drew a flowchart reading
// "You're welcome" -> "Glad that helped" -> "If anything else comes up".
// Nothing asked whether the TURN deserved a picture before staging one.
describe('a turn that is only pleasantries never earns a diagram', () => {
  for (const closing of [
    'Okay, thank you. Thank you',
    'thanks',
    'thank you so much',
    'got it, cheers',
    'alright, that s all',
    'ok bye',
    'perfect, appreciate it',
  ]) {
    test(`"${closing}"`, () => {
      expect(isConversationalTurn(closing)).toBe(true);
    });
  }

  // The guard must not swallow a real request that merely opens politely.
  for (const real of [
    'great, now explain the stages of mitosis',
    'thanks - can you walk me through the water cycle',
    'ok so how did the Japanese and Americans fight in World War II',
    'compare a cat and a dog',
  ]) {
    test(`but not "${real}"`, () => {
      expect(isConversationalTurn(real)).toBe(false);
    });
  }
});

// Field report: "oh" produced a flowchart; a later request that genuinely
// wanted one produced none. Both were this file's predicates, not the model.
describe('a greeting prefix does not disqualify a real request', () => {
  for (const real of [
    'hey, explain the history of the Roman Empire',
    'hi Regent, walk me through how an engine works',
    'hello, compare Ollama and NVIDIA',
    'yo, give me an overview of transformers',
    'good morning, break down the deployment steps',
  ]) {
    test(`"${real}" is a real ask`, () => {
      expect(isConversationalTurn(real)).toBe(false);
      // The filler cue reads the same predicate, so it was suppressed too.
      expect(expectsVisualExplanation(real)).toBe(true);
    });
  }

  // Pins the behavior the fix must not break: a bare greeting is still speech.
  for (const chat of ['hey there', 'hi Regent', 'how are you doing today', 'hello']) {
    test(`"${chat}" is still conversational`, () => {
      expect(isConversationalTurn(chat)).toBe(true);
    });
  }
});

describe('a backchannel earns no last-resort diagram', () => {
  test('"oh" gets no fallback visual', () => {
    expect(
      fallbackPresentSpec('oh', "Yeah? What's on your mind? I'm here if you want to dig in."),
    ).toBeNull();
    expect(fallbackPresentSpec('Oh.', 'Right, it is pretty wild.')).toBeNull();
    expect(fallbackPresentSpec('hmm', 'Take your time.')).toBeNull();
    expect(fallbackPresentSpec('wow', 'It surprised me too. Worth a look.')).toBeNull();
  });

  test('a one-word question is still a real ask', () => {
    const spec = fallbackPresentSpec(
      'why?',
      'Pressure builds first. Then the valve opens. Then the gas escapes.',
    );
    expect(spec?.type).toBe('flow');
  });

  test('one explanation point is not an explanation', () => {
    expect(fallbackPresentSpec('explain the water cycle', 'Water evaporates.')).toBeNull();
  });

  test('no surviving spec carries a fabricated node', () => {
    const spec = fallbackPresentSpec(
      'walk me through the stages of mitosis',
      'Chromosomes condense. The spindle forms. Sister chromatids separate.',
    );
    expect(spec).not.toBeNull();
    expect(JSON.stringify(spec)).not.toContain('"Result"');
  });
});
