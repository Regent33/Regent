import type { Answer, Question } from "@features/chat/domain/questionnaire.ts";
import { isMulti } from "@features/chat/domain/questionnaire.ts";
import {
  answerFromSelection,
  canSubmit,
  EMPTY_SELECTION,
  moveCursor,
  rowsFor,
  type Selection,
  toggle,
} from "@features/chat/domain/selection.ts";
import { QuestionCard } from "@features/chat/presentation/components/QuestionCard.tsx";
// The keyboard owner for a question card. Everything it knows about navigation
// lives in domain/selection.ts as pure functions; this file is the wiring, the
// same split SelectList/SetupWizard already use.
//
// Free text is NOT a second input widget: picking "Something else" (or a `text`
// question) hands the keyboard to the existing MessageInput via `onCustom`, so
// only one useInput is ever live and history/paste/editing come for free.
import { useInput } from "ink";
import { useState } from "react";

interface QuestionPromptProps {
  readonly question: Question;
  readonly step: number;
  readonly total: number;
  /** False while the message input owns the keyboard (free-text answering). */
  readonly isActive: boolean;
  readonly onAnswer: (answer: Answer) => void;
  /** Hand the keyboard to MessageInput for a free-text answer. */
  readonly onCustom: () => void;
  /** Esc — skip this question (or dismiss the card, upstream's call). */
  readonly onSkip: () => void;
}

export function QuestionPrompt({
  question,
  step,
  total,
  isActive,
  onAnswer,
  onCustom,
  onSkip,
}: QuestionPromptProps) {
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);
  const rows = rowsFor(question);
  // A fresh question starts at the top with nothing picked. ChatView keys this
  // component by question id, so React remounts it and the state resets — no
  // effect to keep in sync, and no way for question 2 to inherit question 1's
  // cursor and submit a stale choice.

  useInput(
    (input, key) => {
      if (key.escape) return onSkip();
      if (key.upArrow) return setSelection((s) => moveCursor(s, -1, rows.length));
      if (key.downArrow) return setSelection((s) => moveCursor(s, 1, rows.length));

      // Digits jump to a row. On a single-select that IS the answer — one
      // keystroke, the thing the whole card exists to make possible.
      const digit = /^[1-9]$/.test(input) ? Number(input) - 1 : -1;
      if (digit >= 0 && digit < rows.length) {
        const row = rows[digit];
        if (!row) return;
        if (row.kind === "custom") return onCustom();
        const next = toggle({ ...selection, cursor: digit }, question, row);
        setSelection(next);
        if (!isMulti(question.kind)) onAnswer(answerFromSelection(question, next));
        return;
      }

      const row = rows[selection.cursor];
      if (input === " ") {
        if (row?.kind === "custom") return onCustom();
        if (row) setSelection((s) => toggle(s, question, row));
        return;
      }
      if (key.return) {
        if (row?.kind === "custom") return onCustom();
        if (row && !isMulti(question.kind) && !selection.chosen.includes(row.id)) {
          // Enter on a highlighted row means "this one", not "submit nothing".
          return onAnswer(answerFromSelection(question, toggle(selection, question, row)));
        }
        if (canSubmit(question, selection)) onAnswer(answerFromSelection(question, selection));
      }
    },
    { isActive },
  );

  return (
    <QuestionCard question={question} rows={rows} selection={selection} step={step} total={total} />
  );
}
