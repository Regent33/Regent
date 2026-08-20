// Cursor + selection state for the question card, as pure data. Kept out of
// questionnaire.ts because that file is byte-identical with the Desktop copy
// (scripts/tests/verify-questionnaire-schema.py) — a CLI-only helper there
// would fail the parity gate. Kept out of React because domain imports no
// framework: QuestionPrompt holds one of these in useState and calls these
// functions, so every navigation rule tests without a terminal.

import {
  type Answer,
  allowsCustom,
  isMulti,
  needsOptions,
  optionsOf,
  type Question,
} from "@features/chat/domain/questionnaire.ts";

/** One navigable line of the card. Index into `rowsFor()` is the cursor. */
export type QuestionRow =
  | { readonly kind: "option"; readonly id: string; readonly label: string; readonly hint: string }
  /** The always-present free-text escape hatch ("Something else…"). */
  | { readonly kind: "custom" };

export interface Selection {
  readonly cursor: number;
  /** Chosen option ids in pick order — which is also the ranking for `rank`. */
  readonly chosen: readonly string[];
}

export const EMPTY_SELECTION: Selection = { cursor: 0, chosen: [] };

/** `confirm` navigates like a two-option select; these are its synthetic rows. */
const CONFIRM_OPTIONS = [
  { id: "yes", label: "Yes" },
  { id: "no", label: "No" },
] as const;

/**
 * The rows to draw, in order. `text` questions have none — they escalate
 * straight to the message input rather than pretending to be a list.
 */
export function rowsFor(question: Question): readonly QuestionRow[] {
  if (question.kind === "text") return [];
  const source =
    question.kind === "confirm"
      ? CONFIRM_OPTIONS.map((o) => ({ id: o.id, label: o.label, description: undefined }))
      : optionsOf(question);
  const rows: QuestionRow[] = source.map((o) => ({
    kind: "option",
    id: o.id,
    label: o.label,
    hint: o.description ?? "",
  }));
  // A confirm is already exhaustive — "Something else" on a yes/no is noise.
  if (question.kind !== "confirm" && allowsCustom(question)) rows.push({ kind: "custom" });
  return rows;
}

/** Move the cursor, wrapping at both ends so ↑ from the top reaches the last row. */
export function moveCursor(selection: Selection, delta: number, rowCount: number): Selection {
  if (rowCount === 0) return selection;
  const next = (((selection.cursor + delta) % rowCount) + rowCount) % rowCount;
  return { ...selection, cursor: next };
}

/**
 * Pick the row under the cursor. Single-select replaces; multi-select and rank
 * append (so pick order is the ranking) and un-pick on a second press.
 */
export function toggle(selection: Selection, question: Question, row: QuestionRow): Selection {
  if (row.kind !== "option") return selection;
  if (!isMulti(question.kind)) return { ...selection, chosen: [row.id] };
  const chosen = selection.chosen.includes(row.id)
    ? selection.chosen.filter((id) => id !== row.id)
    : [...selection.chosen, row.id];
  return { ...selection, chosen };
}

/** 1-based pick order for `rank`, or undefined when the option isn't chosen. */
export function rankOf(selection: Selection, optionId: string): number | undefined {
  const at = selection.chosen.indexOf(optionId);
  return at === -1 ? undefined : at + 1;
}

/**
 * Whether Enter may submit. A required question needs a pick; an optional one
 * may be submitted empty, which reads as "no preference" and is answered as a
 * skip rather than as an empty selection the model has to interpret.
 */
export function canSubmit(question: Question, selection: Selection): boolean {
  if (!needsOptions(question.kind) && question.kind !== "confirm") return true;
  return selection.chosen.length > 0 || question.required !== true;
}

/** The typed answer for a completed selection. */
export function answerFromSelection(question: Question, selection: Selection): Answer {
  if (selection.chosen.length === 0) return { kind: "skipped" };
  if (question.kind === "confirm") return { kind: "confirmed", yes: selection.chosen[0] === "yes" };
  return { kind: "selected", option_ids: [...selection.chosen] };
}
