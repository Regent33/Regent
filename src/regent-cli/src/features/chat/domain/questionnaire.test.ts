import { expect, test } from "bun:test";
import {
  answerFor,
  applyAnswer,
  describeAnswer,
  nextUnanswered,
  parseTextAnswer,
  type Question,
  type Questionnaire,
  type QuestionnaireAnswer,
} from "./questionnaire.ts";

// The Desktop copy is proven byte-identical by
// scripts/tests/verify-questionnaire-schema.py, so testing this one tests both.

const pick = (kind: Question["kind"], n = 3): Question => ({
  id: "q1",
  prompt: "Pick one",
  kind,
  options: Array.from({ length: n }, (_, i) => ({
    id: String.fromCharCode(97 + i),
    label: `Option ${String.fromCharCode(97 + i)}`,
  })),
});

const sheet: Questionnaire = {
  id: "q_1",
  questions: [pick("single_select"), { id: "q2", prompt: "Sure?", kind: "confirm" }],
};

test("the stepper walks unanswered questions and stops when done", () => {
  let answers: QuestionnaireAnswer["answers"] = [];
  expect(nextUnanswered(sheet, answers)).toBe(0);

  answers = applyAnswer(answers, "q1", { kind: "selected", option_ids: ["b"] });
  expect(nextUnanswered(sheet, answers)).toBe(1);

  answers = applyAnswer(answers, "q2", { kind: "confirmed", yes: true });
  expect(nextUnanswered(sheet, answers)).toBe(-1);
});

test("re-answering replaces rather than appends", () => {
  const once = applyAnswer([], "q1", { kind: "text", text: "first" });
  const twice = applyAnswer(once, "q1", { kind: "text", text: "second" });
  expect(twice).toHaveLength(1);
  expect(answerFor(twice, "q1")).toEqual({ kind: "text", text: "second" });
  expect(answerFor(twice, "nope")).toBeUndefined();
});

test("text replies map onto typed answers, ambiguity stays free text", () => {
  const single = pick("single_select");
  const multi = pick("multi_select");
  const confirm: Question = { id: "c", prompt: "Sure?", kind: "confirm" };

  expect(parseTextAnswer(single, "2")).toEqual({ kind: "selected", option_ids: ["b"] });
  expect(parseTextAnswer(multi, "1, 3")).toEqual({ kind: "selected", option_ids: ["a", "c"] });
  // Order is the ranking; repeats collapse.
  expect(parseTextAnswer(multi, "3 1 3")).toEqual({ kind: "selected", option_ids: ["c", "a"] });
  expect(parseTextAnswer(confirm, "Yep")).toEqual({ kind: "confirmed", yes: true });
  expect(parseTextAnswer(confirm, "nope")).toEqual({ kind: "confirmed", yes: false });
  expect(parseTextAnswer(single, "   ")).toEqual({ kind: "skipped" });

  for (const reply of ["3 or 4, whichever", "9", "0", "the second one", "1,2"]) {
    expect(parseTextAnswer(single, reply).kind).toBe("text");
  }
});

test("answers describe themselves with labels, unknown ids degrade to the id", () => {
  const q = pick("multi_select");
  expect(describeAnswer(q, { kind: "selected", option_ids: ["a", "c"] })).toBe(
    "Option a, Option c",
  );
  expect(describeAnswer(q, { kind: "selected", option_ids: ["zz"] })).toBe("zz");
  expect(describeAnswer(q, { kind: "skipped" })).toBe("(skipped)");
  expect(describeAnswer(q, { kind: "confirmed", yes: false })).toBe("no");
});
