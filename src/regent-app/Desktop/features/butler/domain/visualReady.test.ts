import { describe, expect, test } from 'bun:test';
import { extractPresentSpec } from '@/shared/diagram/presentSpec';
import {
  createVisualReadyGate,
  expectsVisualExplanation,
  fallbackPresentSpec,
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
