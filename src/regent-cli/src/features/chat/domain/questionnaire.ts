// MIRROR of src/crates/regent-kernel/src/contracts/questionnaire.rs — the
// structured-question contract. Hand-copied because the CLI and the Desktop app
// are separate build roots that cannot import Rust or each other; both copies
// are byte-identical below the header and checked against the Rust by
// scripts/tests/verify-questionnaire-schema.py in the `parity` CI job.
// Edit the Rust first, then both copies, or CI fails.
//
// Pure: types plus the stepper helpers every surface needs. No I/O, no UI.

export type QuestionKind = "single_select" | "multi_select" | "text" | "confirm" | "rank";

export interface QuestionOption {
  readonly id: string;
  readonly label: string;
  readonly description?: string;
}

export interface Question {
  readonly id: string;
  readonly prompt: string;
  /** Very short chip label beside the question ("Auth method", "Scope"). */
  readonly header?: string;
  readonly kind: QuestionKind;
  readonly options?: readonly QuestionOption[];
  /** "Something else" free-text row. Defaults to true on the wire. */
  readonly allow_custom?: boolean;
  readonly required?: boolean;
}

export interface Questionnaire {
  readonly id: string;
  readonly questions: readonly Question[];
}

export type Answer =
  /** Option ids in the order chosen — which is also the ranking for `rank`. */
  | { readonly kind: "selected"; readonly option_ids: readonly string[] }
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "confirmed"; readonly yes: boolean }
  | { readonly kind: "skipped" };

export interface QuestionnaireAnswer {
  readonly questionnaire_id: string;
  /** One entry per answered question, keyed by `Question.id`. */
  readonly answers: readonly (readonly [string, Answer])[];
  readonly cancelled?: boolean;
}

/** True when the kind is answered by picking from `options`. */
export function needsOptions(kind: QuestionKind): boolean {
  return kind === "single_select" || kind === "multi_select" || kind === "rank";
}

/** True when more than one option may be chosen. */
export function isMulti(kind: QuestionKind): boolean {
  return kind === "multi_select" || kind === "rank";
}

/** Options with a guaranteed array, so callers never re-check for undefined. */
export function optionsOf(question: Question): readonly QuestionOption[] {
  return question.options ?? [];
}

/** The free-text row is on unless the questionnaire explicitly turns it off. */
export function allowsCustom(question: Question): boolean {
  return question.allow_custom !== false;
}

/** Index of the first question with no answer yet, or -1 when all are done. */
export function nextUnanswered(
  questionnaire: Questionnaire,
  answers: QuestionnaireAnswer["answers"],
): number {
  const answered = new Set(answers.map(([id]) => id));
  return questionnaire.questions.findIndex((q) => !answered.has(q.id));
}

/** Records one answer, replacing any earlier answer for the same question. */
export function applyAnswer(
  answers: QuestionnaireAnswer["answers"],
  questionId: string,
  answer: Answer,
): QuestionnaireAnswer["answers"] {
  const kept = answers.filter(([id]) => id !== questionId);
  return [...kept, [questionId, answer] as const];
}

/** The answer recorded for `questionId`, if any. */
export function answerFor(
  answers: QuestionnaireAnswer["answers"],
  questionId: string,
): Answer | undefined {
  return answers.find(([id]) => id === questionId)?.[1];
}

/** Human-readable summary of one answer, for a transcript line or a chip. */
export function describeAnswer(question: Question, answer: Answer): string {
  switch (answer.kind) {
    case "selected":
      return answer.option_ids
        .map((id) => optionsOf(question).find((o) => o.id === id)?.label ?? id)
        .join(", ");
    case "text":
      return answer.text;
    case "confirmed":
      return answer.yes ? "yes" : "no";
    case "skipped":
      return "(skipped)";
  }
}

/**
 * Maps one free-text reply onto a typed answer — the non-TTY / piped path, and
 * the mirror of `parse_text_answer` in the Rust. `2`, `1,3`, `yes`/`no`, or
 * anything else as custom text. Ambiguity stays free text rather than silently
 * meaning an option.
 */
export function parseTextAnswer(question: Question, reply: string): Answer {
  const text = reply.trim();
  if (text === "") return { kind: "skipped" };
  if (question.kind === "confirm") {
    const yes = affirmative(text);
    return yes === undefined ? { kind: "text", text } : { kind: "confirmed", yes };
  }
  if (needsOptions(question.kind)) {
    const ids = parseIndices(question, text);
    if (ids) return { kind: "selected", option_ids: ids };
  }
  return { kind: "text", text };
}

function parseIndices(question: Question, text: string): string[] | undefined {
  const tokens = text
    .split(/[,;\s]+/)
    .map((t) => t.trim())
    .filter((t) => t !== "");
  if (tokens.length === 0) return undefined;
  if (tokens.length > 1 && !isMulti(question.kind)) return undefined;
  const options = optionsOf(question);
  const ids: string[] = [];
  for (const token of tokens) {
    if (!/^\d+$/.test(token)) return undefined;
    const option = options[Number(token) - 1];
    if (!option) return undefined;
    if (!ids.includes(option.id)) ids.push(option.id);
  }
  return ids;
}

const YES = new Set(["y", "yes", "yeah", "yep", "ok", "okay", "sure", "1", "true"]);
const NO = new Set(["n", "no", "nope", "nah", "2", "false"]);

function affirmative(text: string): boolean | undefined {
  const word = text.trim().toLowerCase();
  if (YES.has(word)) return true;
  if (NO.has(word)) return false;
  return undefined;
}
