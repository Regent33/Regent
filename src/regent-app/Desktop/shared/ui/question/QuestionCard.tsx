'use client';
// The structured `ask_user` card. It sits IN the transcript flow, like the
// approval row and unlike a modal, so earlier scrollback stays readable and a
// second card can follow the first.
//
// One `question.request` carries every question, so the stepper is local state
// here: "first question with no answer yet" (nextUnanswered) rather than a
// counter that can drift from the answers it indexes. The reducer only stores
// the questionnaire and, once, the finished answer — which is what turns this
// card into its own scrollback summary.
//
// Navigation rules live in shared/kernel/selection.ts (pure) and the
// keyboard in useQuestionKeys.ts, so this file is layout and wiring.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { CloseIcon } from '@/shared/ui/icons';
import {
  type Answer,
  type Questionnaire,
  type QuestionnaireAnswer,
  applyAnswer,
  describeAnswer,
  isMulti,
  nextUnanswered,
} from '@/shared/kernel/questionnaire';
import {
  EMPTY_SELECTION,
  type Selection,
  answerFromSelection,
  canSubmit,
  moveCursor,
  rankOf,
  rowsFor,
  toggle,
} from '@/shared/kernel/selection';
import { CustomInputRow } from '@/shared/ui/question/CustomInputRow';
import { OptionRow } from '@/shared/ui/question/OptionRow';
import { useQuestionKeys } from '@/shared/ui/question/useQuestionKeys';

export interface QuestionCardProps {
  questionnaire: Questionnaire;
  answered?: QuestionnaireAnswer;
  onRespond?: (answer: QuestionnaireAnswer) => void;
}

export function QuestionCard({ questionnaire, answered, onRespond }: QuestionCardProps) {
  const s = t().chat.question;
  const [answers, setAnswers] = useState<QuestionnaireAnswer['answers']>([]);
  const [customOpen, setCustomOpen] = useState(false);
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);

  const total = questionnaire.questions.length;
  const at = nextUnanswered(questionnaire, answers);
  const question = at >= 0 ? questionnaire.questions[at] : undefined;
  // Answered locally counts as answered even before the reducer's stamp comes
  // back, so the card never flashes an empty question after the last pick.
  const done =
    answered ?? (question === undefined ? { questionnaire_id: questionnaire.id, answers } : undefined);
  const rows = question === undefined ? [] : rowsFor(question);
  const multi = question !== undefined && isMulti(question.kind);
  const boxOpen = customOpen || question?.kind === 'text';
  const rowId = (index: number) => `${questionnaire.id}-${at}-${index}`;

  const respond = (final: QuestionnaireAnswer['answers'], cancelled: boolean) =>
    onRespond?.({ questionnaire_id: questionnaire.id, answers: final, cancelled });

  const answer = (value: Answer) => {
    if (question === undefined) return;
    const next = applyAnswer(answers, question.id, value);
    setAnswers(next);
    setSelection(EMPTY_SELECTION);
    setCustomOpen(false);
    if (nextUnanswered(questionnaire, next) === -1) respond(next, false);
  };

  const pick = (index: number) => {
    const row = rows[index];
    if (row === undefined || question === undefined) return;
    setSelection((sel) => ({ ...sel, cursor: index }));
    if (row.kind === 'custom') {
      setCustomOpen(true);
      return;
    }
    const next = toggle({ ...selection, cursor: index }, question, row);
    setSelection(next);
    // On a single-select the pick IS the answer — one keystroke, one click.
    if (!isMulti(question.kind)) answer(answerFromSelection(question, next));
  };

  const submit = () => {
    const row = rows[selection.cursor];
    if (question === undefined) return;
    if (row?.kind === 'custom') {
      setCustomOpen(true);
      return;
    }
    // Enter on a highlighted row means "this one", not "submit nothing".
    if (row !== undefined && !multi && !selection.chosen.includes(row.id)) {
      answer(answerFromSelection(question, toggle(selection, question, row)));
      return;
    }
    if (canSubmit(question, selection)) answer(answerFromSelection(question, selection));
  };

  const skippable = question !== undefined && question.required !== true;
  const keys = useQuestionKeys({
    rowCount: rows.length,
    active: done === undefined && !boxOpen,
    onMove: (delta) => setSelection((sel) => moveCursor(sel, delta, rows.length)),
    onJump: pick,
    onToggle: () => pick(selection.cursor),
    onSubmit: submit,
    onEscape: skippable ? () => answer({ kind: 'skipped' }) : undefined,
  });

  if (done !== undefined) {
    return (
      <div className="rounded-md bg-hover px-3 py-2.5">
        <p className="text-xs font-semibold text-text-primary">{s.title(total)}</p>
        {done.cancelled === true ? (
          <p className="mt-0.5 text-xs text-text-tertiary">{s.dismissed}</p>
        ) : (
          <ul className="mt-1 space-y-0.5">
            {done.answers.map(([id, value]) => {
              const q = questionnaire.questions.find((x) => x.id === id);
              if (q === undefined) return null;
              return (
                <li key={id} className="break-words text-xs text-text-secondary">
                  {s.answerLine(q.prompt, describeAnswer(q, value))}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    );
  }

  const legend = question?.kind === 'rank' ? s.hintRank : multi ? s.hintMulti : undefined;

  return (
    <div
      ref={keys.ref}
      tabIndex={0}
      role={multi ? 'group' : 'radiogroup'}
      aria-labelledby={`${questionnaire.id}-prompt`}
      aria-activedescendant={rows.length > 0 ? rowId(selection.cursor) : undefined}
      onKeyDown={keys.onKeyDown}
      className="rounded-md bg-hover px-3 py-2.5"
    >
      <div className="flex items-start gap-2">
        <p className="min-w-0 flex-1 text-xs font-semibold text-text-primary">{s.title(total)}</p>
        {total > 1 && (
          <span aria-live="polite" className="shrink-0 text-[11px] text-text-tertiary">
            {s.step(at + 1, total)}
          </span>
        )}
        <button
          type="button"
          aria-label={s.dismiss}
          title={s.dismiss}
          onClick={() => respond([], true)}
          className="-mr-1 flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-sm text-text-tertiary hover:bg-surface hover:text-text-primary"
        >
          <CloseIcon className="size-3.5" />
        </button>
      </div>
      {/* The short field label ("Auth method", "Scope"). It is what makes the
          card read as a form field rather than a paragraph, which is why the
          contract carries it and the CLI already draws it — this surface was
          dropping it silently. Model-authored, so it is text, never markdown. */}
      {question?.header !== undefined && question.header !== '' && (
        <span className="mt-1.5 inline-block max-w-full truncate rounded-md border border-border-subtle bg-surface-raised px-1.5 py-0.5 text-[11px] text-text-secondary">
          {question.header}
        </span>
      )}
      {/* Model-authored text, rendered as text — never markdown (§9). */}
      <p id={`${questionnaire.id}-prompt`} className="mt-1 break-words text-sm text-text-primary">
        {question?.prompt}
      </p>
      {legend !== undefined && <p className="mt-0.5 text-[11px] text-text-tertiary">{legend}</p>}
      <div className="mt-2 space-y-1">
        {rows.map((row, index) =>
          row.kind === 'custom' ? (
            <CustomInputRow
              key="custom"
              id={rowId(index)}
              index={index}
              multi={multi}
              cursor={selection.cursor === index}
              open={boxOpen}
              onOpen={() => pick(index)}
              onSubmit={(text) => answer({ kind: 'text', text })}
              onCancel={() => setCustomOpen(false)}
            />
          ) : (
            <OptionRow
              key={row.id}
              id={rowId(index)}
              index={index}
              label={
                question?.kind === 'confirm' ? (row.id === 'yes' ? s.confirmYes : s.confirmNo) : row.label
              }
              description={row.hint}
              multi={multi}
              checked={selection.chosen.includes(row.id)}
              rank={question?.kind === 'rank' ? rankOf(selection, row.id) : undefined}
              cursor={selection.cursor === index}
              onSelect={() => pick(index)}
            />
          ),
        )}
        {rows.length === 0 && (
          <CustomInputRow
            id={rowId(0)}
            multi={false}
            cursor={false}
            open
            onOpen={() => undefined}
            onSubmit={(text) => answer({ kind: 'text', text })}
            onCancel={() => undefined}
          />
        )}
      </div>
      {(multi || skippable) && (
        <div className="mt-2 flex gap-2">
          {multi && (
            <Button size="sm" disabled={!canSubmit(question, selection)} onClick={submit}>
              {s.submit}
            </Button>
          )}
          {skippable && (
            <Button variant="secondary" size="sm" onClick={() => answer({ kind: 'skipped' })}>
              {s.skip}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
