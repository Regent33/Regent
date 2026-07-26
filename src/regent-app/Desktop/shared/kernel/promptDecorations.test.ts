import { describe, expect, test } from 'bun:test';
import { splitPromptDecorations } from '@/shared/kernel/promptDecorations';

describe('splitPromptDecorations', () => {
  test('leaves an ordinary message untouched', () => {
    expect(splitPromptDecorations('what does this do?')).toEqual({
      text: 'what does this do?',
      attachments: [],
    });
  });

  test('lifts attachment refs out and keeps the name only', () => {
    const stored =
      'summarize these\n\n[attached file: C:\\Users\\me\\.regent\\attachments\\s1\\notes.pdf]\n' +
      '[attached file: /home/me/.regent/attachments/s1/chart.png]';
    expect(splitPromptDecorations(stored)).toEqual({
      text: 'summarize these',
      attachments: ['notes.pdf', 'chart.png'],
    });
  });

  test('survives an attachment-only message', () => {
    expect(splitPromptDecorations('[attached file: /a/b/report.xlsx]')).toEqual({
      text: '',
      attachments: ['report.xlsx'],
    });
  });

  test('does not eat prose that merely mentions the phrase', () => {
    const line = 'the log said [attached file: x] somewhere mid-sentence';
    expect(splitPromptDecorations(line).attachments).toEqual([]);
  });

  // The reported bug: the deacon's editor note rendered as prose in the bubble.
  test('lifts a selected-folder note out of the message', () => {
    const stored = 'what does this do?\n\n[The user has the .agents folder selected.]';
    expect(splitPromptDecorations(stored)).toEqual({
      text: 'what does this do?',
      attachments: [],
      context: '.agents',
    });
  });

  test('lifts an open-file note out and labels it by file name', () => {
    const stored = 'explain\n\n[The user has D:\\repo\\src\\main.rs open in the editor.]';
    const out = splitPromptDecorations(stored);
    expect(out.text).toBe('explain');
    expect(out.context).toBe('main.rs');
  });

  test('a multi-line selection note is cut whole, with its line range kept', () => {
    const stored =
      'fix this\n\n[The user is looking at src/app.ts in the editor and has lines 10-14 ' +
      'selected:\nconst a = 1;\nconst b = 2;\n]';
    const out = splitPromptDecorations(stored);
    expect(out.text).toBe('fix this');
    expect(out.context).toBe('app.ts · 10-14');
  });

  test('attachments and an editor note coexist', () => {
    const stored =
      'look\n\n[attached file: /a/b/spec.pdf]\n\n[The user has the src folder selected.]';
    expect(splitPromptDecorations(stored)).toEqual({
      text: 'look',
      attachments: ['spec.pdf'],
      context: 'src',
    });
  });

  test('prose that merely starts like a note is not eaten', () => {
    const stored = 'ok\n\n[The user manual says to press enter]';
    const out = splitPromptDecorations(stored);
    expect(out.text).toBe('ok\n\n[The user manual says to press enter]');
    expect(out.context).toBeUndefined();
  });
});
