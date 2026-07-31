// T.1/T.2 are exactly the kind of change that regresses silently, and the whole
// point of keeping the editing model pure is that it can be tested without a
// terminal, a PTY or a rendering library.
import { expect, test } from "bun:test";
import { applyKey, at, BINDINGS, type Composer, EMPTY } from "./composer.ts";

/** Type a sequence of [input, key] pairs and return the final state. */
function type(start: Composer, keys: Array<[string, Record<string, boolean>]>): Composer {
  let s = start;
  for (const [input, key] of keys) {
    const next = applyKey(s, input, key);
    if (next !== null && next !== "submit") s = next;
  }
  return s;
}

test("printable input inserts at the cursor", () => {
  const s = type(EMPTY, [
    ["a", {}],
    ["b", {}],
    ["c", {}],
  ]);
  expect(s.value).toBe("abc");
  expect(s.pos).toBe(3);
  const moved = applyKey(at("abc", 1), "X", {});
  expect(moved).toEqual({ value: "aXbc", pos: 2, pasting: false });
});

test("plain enter submits; alt+enter and shift+enter insert a newline", () => {
  expect(applyKey(at("hi", 2), "", { return: true })).toBe("submit");
  expect(applyKey(at("hi", 2), "", { return: true, meta: true })).toEqual({
    value: "hi\n",
    pos: 3,
    pasting: false,
  });
  expect(applyKey(at("hi", 2), "", { return: true, shift: true })).toEqual({
    value: "hi\n",
    pos: 3,
    pasting: false,
  });
});

// The defect this whole task exists for: a pasted stack trace used to be broken
// up by its own newlines instead of arriving as one message.
test("a bracketed paste is one value, newlines and all", () => {
  const s = applyKey(EMPTY, "[200~line one\nline two\nline three[201~", {});
  expect(s).not.toBe("submit");
  if (s === "submit" || s === null) throw new Error("paste must produce a value");
  expect(s.value).toBe("line one\nline two\nline three");
  expect(s.pasting).toBe(false);
});

test("a paste split across chunks stays one value and never submits mid-way", () => {
  // Large pastes arrive as several stdin reads; the end marker may be chunks later.
  let s = type(EMPTY, [["[200~first line\n", {}]]);
  expect(s.pasting).toBe(true);
  // A newline arriving as its own chunk mid-paste must NOT submit.
  expect(applyKey(s, "", { return: true })).not.toBe("submit");
  s = type(s, [
    ["second line\n", { return: true }],
    ["third[201~", {}],
  ]);
  expect(s.value).toBe("first line\nsecond line\nthird");
  expect(s.pasting).toBe(false);
  // And once the paste ends, enter submits again.
  expect(applyKey(s, "", { return: true })).toBe("submit");
});

test("pasted CRLF and bare CR become plain newlines", () => {
  const s = applyKey(EMPTY, "[200~a\r\nb\rc[201~", {});
  if (s === "submit" || s === null) throw new Error("paste must produce a value");
  expect(s.value).toBe("a\nb\nc");
});

test("ctrl+a/e and home/end work per LINE, not per buffer", () => {
  const s = at("first\nsecond", 8); // inside "second"
  expect(applyKey(s, "a", { ctrl: true })).toMatchObject({ pos: 6 });
  expect(applyKey(s, "e", { ctrl: true })).toMatchObject({ pos: 12 });
  expect(applyKey(s, "", { home: true })).toMatchObject({ pos: 6 });
  expect(applyKey(s, "", { end: true })).toMatchObject({ pos: 12 });
});

test("ctrl+w kills the word before the cursor, ctrl+u to line start, ctrl+k to line end", () => {
  expect(applyKey(at("one two three", 13), "w", { ctrl: true })).toMatchObject({
    value: "one two ",
    pos: 8,
  });
  // Trailing spaces go with the word, as readline does.
  expect(applyKey(at("one two   ", 10), "w", { ctrl: true })).toMatchObject({ value: "one " });
  expect(applyKey(at("one two", 4), "u", { ctrl: true })).toMatchObject({ value: "two", pos: 0 });
  expect(applyKey(at("one two", 4), "k", { ctrl: true })).toMatchObject({ value: "one ", pos: 4 });
  // On a later line, ctrl+u must not eat the line above.
  expect(applyKey(at("keep\nkill me", 9), "u", { ctrl: true })).toMatchObject({
    value: "keep\n me",
  });
});

test("ctrl+arrows move by word, plain arrows by character", () => {
  expect(applyKey(at("one two", 7), "", { leftArrow: true, ctrl: true })).toMatchObject({ pos: 4 });
  expect(applyKey(at("one two", 0), "", { rightArrow: true, ctrl: true })).toMatchObject({
    pos: 3,
  });
  expect(applyKey(at("one two", 7), "", { leftArrow: true })).toMatchObject({ pos: 6 });
  // Cursor movement stops at the ends rather than wrapping or going negative.
  expect(applyKey(at("x", 0), "", { leftArrow: true })).toMatchObject({ pos: 0 });
  expect(applyKey(at("x", 1), "", { rightArrow: true })).toMatchObject({ pos: 1 });
});

test("up/down move between lines, and yield to history when there is no line", () => {
  const s = at("first\nsecond", 8); // column 2 of line 2
  expect(applyKey(s, "", { upArrow: true })).toMatchObject({ pos: 2 });
  // Nothing above the first line / below the last → null, so the component
  // falls back to history recall. That fallback is the whole contract here.
  expect(applyKey(at("only one line", 3), "", { upArrow: true })).toBeNull();
  expect(applyKey(at("only one line", 3), "", { downArrow: true })).toBeNull();
  expect(applyKey(at("a\nbb", 0), "", { downArrow: true })).toMatchObject({ pos: 2 });
});

test("moving down onto a shorter line clamps to its end", () => {
  expect(applyKey(at("longer line\nab", 9), "", { downArrow: true })).toMatchObject({ pos: 14 });
});

test("backspace and delete both erase backwards, and do nothing at the start", () => {
  expect(applyKey(at("abc", 3), "", { backspace: true })).toMatchObject({ value: "ab", pos: 2 });
  expect(applyKey(at("abc", 3), "", { delete: true })).toMatchObject({ value: "ab", pos: 2 });
  expect(applyKey(at("abc", 0), "", { backspace: true })).toMatchObject({ value: "abc", pos: 0 });
});

test("every binding in the overlay names keys and an action", () => {
  // The overlay and the input read the same table, so it cannot document a key
  // that does not exist — but an empty row would still be a silent gap.
  expect(BINDINGS.length).toBeGreaterThan(8);
  for (const [keys, what] of BINDINGS) {
    expect(keys.trim()).not.toBe("");
    expect(what.trim()).not.toBe("");
  }
});
