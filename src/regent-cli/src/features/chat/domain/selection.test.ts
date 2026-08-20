import { describe, expect, test } from "bun:test";
import type { Question } from "@features/chat/domain/questionnaire.ts";
import {
  answerFromSelection,
  canSubmit,
  EMPTY_SELECTION,
  moveCursor,
  rankOf,
  rowsFor,
  toggle,
} from "@features/chat/domain/selection.ts";

/** The row at `i`, or a failed test — an out-of-range index here is the bug. */
const row = (question: Question, i: number) => {
  const found = rowsFor(question)[i];
  if (!found) throw new Error(`no row ${i}`);
  return found;
};

const q = (over: Partial<Question>): Question => ({
  id: "q1",
  prompt: "Which?",
  kind: "single_select",
  options: [
    { id: "a", label: "Alpha" },
    { id: "b", label: "Beta" },
  ],
  ...over,
});

// Row indices ARE the cursor and the digit shortcuts, so a wrong row set is a
// wrong answer, not a cosmetic bug.
describe("rowsFor", () => {
  test("appends the free-text row by default", () => {
    expect(rowsFor(q({})).map((r) => r.kind)).toEqual(["option", "option", "custom"]);
  });

  test("omits it when the questionnaire turns it off", () => {
    expect(rowsFor(q({ allow_custom: false })).map((r) => r.kind)).toEqual(["option", "option"]);
  });

  test("confirm gets synthetic yes/no rows and no free-text escape", () => {
    expect(rowsFor(q({ kind: "confirm", options: [] }))).toEqual([
      { kind: "option", id: "yes", label: "Yes", hint: "" },
      { kind: "option", id: "no", label: "No", hint: "" },
    ]);
  });

  test("a text question has no rows — it escalates to the message input", () => {
    expect(rowsFor(q({ kind: "text", options: [] }))).toEqual([]);
  });
});

describe("moveCursor", () => {
  test("wraps at both ends", () => {
    expect(moveCursor({ cursor: 0, chosen: [] }, -1, 3).cursor).toBe(2);
    expect(moveCursor({ cursor: 2, chosen: [] }, 1, 3).cursor).toBe(0);
  });

  test("is a no-op with no rows", () => {
    expect(moveCursor(EMPTY_SELECTION, 1, 0)).toBe(EMPTY_SELECTION);
  });
});

describe("toggle", () => {
  test("single-select replaces the previous pick", () => {
    const question = q({});
    let sel = toggle(EMPTY_SELECTION, question, row(question, 0));
    sel = toggle(sel, question, row(question, 1));
    expect(sel.chosen).toEqual(["b"]);
  });

  test("multi-select accumulates and un-picks on a second press", () => {
    const question = q({ kind: "multi_select" });
    let sel = toggle(
      toggle(EMPTY_SELECTION, question, row(question, 0)),
      question,
      row(question, 1),
    );
    expect(sel.chosen).toEqual(["a", "b"]);
    sel = toggle(sel, question, row(question, 0));
    expect(sel.chosen).toEqual(["b"]);
  });

  test("rank keeps pick order, which is the ranking", () => {
    const question = q({ kind: "rank" });
    const sel = toggle(
      toggle(EMPTY_SELECTION, question, row(question, 1)),
      question,
      row(question, 0),
    );
    expect(sel.chosen).toEqual(["b", "a"]);
    expect(rankOf(sel, "b")).toBe(1);
    expect(rankOf(sel, "a")).toBe(2);
    expect(rankOf(sel, "zzz")).toBeUndefined();
  });

  test("the free-text row never lands in the selection", () => {
    const question = q({});
    const custom = row(question, 2);
    expect(toggle(EMPTY_SELECTION, question, custom).chosen).toEqual([]);
  });
});

describe("canSubmit", () => {
  test("a required question needs a pick", () => {
    expect(canSubmit(q({ required: true }), EMPTY_SELECTION)).toBe(false);
    expect(canSubmit(q({ required: true }), { cursor: 0, chosen: ["a"] })).toBe(true);
  });

  test("an optional one may be submitted empty", () => {
    expect(canSubmit(q({}), EMPTY_SELECTION)).toBe(true);
  });

  test("a text question is always submittable — the input validates it", () => {
    expect(canSubmit(q({ kind: "text", options: [] }), EMPTY_SELECTION)).toBe(true);
  });
});

describe("answerFromSelection", () => {
  test("an empty selection is a skip, not an empty pick list", () => {
    expect(answerFromSelection(q({}), EMPTY_SELECTION)).toEqual({ kind: "skipped" });
  });

  test("confirm maps its synthetic rows to a typed yes/no", () => {
    const question = q({ kind: "confirm", options: [] });
    expect(answerFromSelection(question, { cursor: 0, chosen: ["yes"] })).toEqual({
      kind: "confirmed",
      yes: true,
    });
    expect(answerFromSelection(question, { cursor: 1, chosen: ["no"] })).toEqual({
      kind: "confirmed",
      yes: false,
    });
  });

  test("selects carry the ids in pick order", () => {
    expect(answerFromSelection(q({ kind: "rank" }), { cursor: 0, chosen: ["b", "a"] })).toEqual({
      kind: "selected",
      option_ids: ["b", "a"],
    });
  });
});
