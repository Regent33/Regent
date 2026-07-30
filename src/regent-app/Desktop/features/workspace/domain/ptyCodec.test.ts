import { describe, expect, test } from 'bun:test';
import { decodeOutput, encodeInput } from '@/features/workspace/domain/ptyCodec';

describe('pty codec', () => {
  test('plain typing round-trips', () => {
    const encoded = encodeInput('echo hi\r');
    expect(new TextDecoder().decode(decodeOutput(encoded))).toBe('echo hi\r');
  });

  // The reason base64 is here for INPUT: Ctrl+C is byte 0x03. It has to reach
  // the shell as 0x03, not as an escaped JSON sequence or a dropped character.
  // This version of bun:test takes no per-assertion message, so the identity of
  // a failing byte is carried in the compared value instead.
  test('control bytes survive', () => {
    const codes = ['\x03', '\x04', '\x1b', '\x00'].map((byte) => {
      const bytes = decodeOutput(encodeInput(byte));
      return `${bytes.length}:${bytes[0]}`;
    });
    expect(codes).toEqual(['1:3', '1:4', '1:27', '1:0']);
  });

  test('an ANSI escape sequence survives intact', () => {
    const dsrReply = '\x1b[24;80R';
    expect(new TextDecoder().decode(decodeOutput(encodeInput(dsrReply)))).toBe(dsrReply);
  });

  // Multi-byte input, e.g. a pasted emoji or non-Latin text.
  test('multi-byte characters round-trip', () => {
    const text = 'echo "日本語 😀"\r';
    expect(new TextDecoder().decode(decodeOutput(encodeInput(text)))).toBe(text);
  });

  // A big paste is exactly when a naive String.fromCharCode(...bytes) throws
  // "too many arguments", so the chunked build is load-bearing rather than tidy.
  test('a large paste does not blow the argument limit', () => {
    const huge = 'x'.repeat(500_000);
    const bytes = decodeOutput(encodeInput(huge));
    expect(bytes.length).toBe(500_000);
  });

  // decodeOutput returns BYTES so xterm can do incremental UTF-8 decoding: a
  // character split across two pty.data messages only renders correctly if each
  // half arrives as bytes rather than being string-decoded in isolation.
  test('output decodes to bytes, not a string', () => {
    const bytes = decodeOutput(encodeInput('😀'));
    expect(bytes.constructor.name).toBe('Uint8Array');
    expect(bytes.length).toBe(4);
  });
});
