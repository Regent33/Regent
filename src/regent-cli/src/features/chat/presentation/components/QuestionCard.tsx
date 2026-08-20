import { COPY } from "@app/config/brand.ts";
import type { Question } from "@features/chat/domain/questionnaire.ts";
import { isMulti } from "@features/chat/domain/questionnaire.ts";
import type { QuestionRow, Selection } from "@features/chat/domain/selection.ts";
import { rankOf } from "@features/chat/domain/selection.ts";
import { SelectList, type SelectRow } from "@shared/ui/components/SelectList.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
// One question of a structured `ask_user` card. Render-only — the parent owns
// useInput, the same contract SelectList already has — so every navigation
// rule is testable as pure data (domain/selection.ts) rather than as a
// terminal. Option labels render as plain Text, never markdown: they come from
// the model and this is the injection boundary.
import { Box, Text } from "ink";

interface QuestionCardProps {
  readonly question: Question;
  readonly rows: readonly QuestionRow[];
  readonly selection: Selection;
  /** 1-based position in the questionnaire, for the "2 of 3" chip. */
  readonly step: number;
  readonly total: number;
}

/**
 * The marker in front of a row. Rank shows its pick order (`1.`), multi-select
 * a checkbox, single-select a bullet — so the card says what it expects
 * without a legend.
 */
function marker(question: Question, row: QuestionRow, selection: Selection): string {
  if (row.kind === "custom") return "  ";
  if (question.kind === "rank") {
    const at = rankOf(selection, row.id);
    return at === undefined ? "· " : `${at}.`;
  }
  const on = selection.chosen.includes(row.id);
  if (isMulti(question.kind)) return on ? "[✓]" : "[ ]";
  return on ? "(•)" : "( )";
}

export function QuestionCard({ question, rows, selection, step, total }: QuestionCardProps) {
  const displayRows: SelectRow[] = rows.map((row, i) => ({
    label:
      row.kind === "custom"
        ? `${marker(question, row, selection)} ${COPY.questionCustomRow}`
        : `${marker(question, row, selection)} ${i + 1}. ${row.label}`,
    hint: row.kind === "custom" ? "" : row.hint,
  }));

  return (
    <Box flexDirection="column">
      <Text>
        <Text bold color={palette.gold}>
          ⁇{" "}
        </Text>
        {question.header ? <Text color={palette.tealDim}>[{question.header}] </Text> : null}
        <Text bold color={palette.white}>
          {question.prompt}
        </Text>
        <Text color={palette.grey}>{COPY.questionStep(step, total)}</Text>
      </Text>
      {displayRows.length > 0 ? (
        <Box marginTop={1} flexDirection="column">
          <SelectList rows={displayRows} selected={selection.cursor} />
        </Box>
      ) : null}
      <Text color={palette.grey}>
        {" "}
        {isMulti(question.kind) ? COPY.questionKeysMulti : COPY.questionKeysSingle}
      </Text>
    </Box>
  );
}
