// Base64 for the pty wire, both directions.
//
// Why not JSON strings: pty traffic is arbitrary BYTES. Output carries escape
// sequences and whatever encoding the running program emits, and a multi-byte
// character split across a read boundary is not valid UTF-8 — a JSON string
// cannot hold it without corruption. Input carries control bytes: Ctrl+C is
// 0x03, Ctrl+D is 0x04. Base64 costs ~33% and loses nothing.
//
// Pure, and here rather than in the component, so the round-trip is testable
// without a DOM (this repo's tests have no jsdom).

/** Keystrokes → base64 for `pty.write`. */
export function encodeInput(text: string): string {
  const bytes = new TextEncoder().encode(text);
  // `btoa` takes a "binary string": one char per byte. Built by chunk rather
  // than one big spread — String.fromCharCode(...bytes) blows the argument limit
  // on a large paste, which is exactly when someone pastes a wall of text.
  let binary = '';
  const CHUNK = 8192;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

/** `pty.data` base64 → bytes for xterm.
 *
 * Returns BYTES, not a string, deliberately: xterm's `write` accepts
 * `Uint8Array` and does its own incremental UTF-8 decoding, which is what makes
 * a character split across two `pty.data` messages render correctly. Decoding to
 * a string here would corrupt exactly that case — the one base64 exists to
 * protect.
 */
export function decodeOutput(payload: string): Uint8Array {
  const binary = atob(payload);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
