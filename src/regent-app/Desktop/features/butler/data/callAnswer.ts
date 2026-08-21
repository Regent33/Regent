// Hand a tapped question-card answer back to the paused turn.
//
// It goes to the voice server rather than straight to the deacon because the
// call owns the deacon connection: Butler holds ONE session for the life of
// the server, and `/call/answer` is the endpoint that knows which session that
// is. Token-gated exactly like `/call/turn` — this resumes an agent turn.
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { fetchCallToken } from '@/features/butler/data/speechClient';
import type { QuestionnaireAnswer } from '@/shared/kernel/questionnaire';

/**
 * True when the answer reached a turn that was actually waiting. False means
 * nothing was pending — the question timed out, or a second tap raced the
 * first — which the caller shows rather than swallowing, since the card is
 * about to come down either way.
 */
export async function sendQuestionAnswer(answer: QuestionnaireAnswer): Promise<boolean> {
  try {
    const res = await fetch(`${SPEECH_URL}/call/answer`, {
      method: 'POST',
      body: JSON.stringify(answer),
      headers: {
        'content-type': 'application/json',
        'x-call-token': await fetchCallToken(),
      },
    });
    if (!res.ok) return false;
    const body = (await res.json()) as { resolved?: unknown };
    return body.resolved === true;
  } catch {
    // The server going away mid-answer is the same outcome as a timeout: the
    // turn is not resuming from this tap. Answering out loud still works.
    return false;
  }
}
