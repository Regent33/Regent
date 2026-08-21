// What a finished question card leaves in the scrollback: the prompts and the
// answers that were given, or a single line when the card was dismissed. Split
// out of QuestionCard so the live card stays under the file-size limit.
//
// Prompts and option labels are model-authored and render as TEXT, never
// markdown (§9).
import { t } from '@/shared/i18n/t';
import {
  type Questionnaire,
  type QuestionnaireAnswer,
  describeAnswer,
} from '@/shared/kernel/questionnaire';

export function QuestionSummary({
  questionnaire,
  answered,
}: {
  questionnaire: Questionnaire;
  answered: QuestionnaireAnswer;
}) {
  const s = t().chat.question;
  return (
    <div className="rounded-md bg-hover px-3 py-2.5">
      <p className="text-xs font-semibold text-text-primary">
        {s.title(questionnaire.questions.length)}
      </p>
      {answered.cancelled === true ? (
        <p className="mt-0.5 text-xs text-text-tertiary">{s.dismissed}</p>
      ) : (
        <ul className="mt-1 space-y-0.5">
          {answered.answers.map(([id, value]) => {
            const question = questionnaire.questions.find((x) => x.id === id);
            if (question === undefined) return null;
            return (
              <li key={id} className="break-words text-xs text-text-secondary">
                {s.answerLine(question.prompt, describeAnswer(question, value))}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
