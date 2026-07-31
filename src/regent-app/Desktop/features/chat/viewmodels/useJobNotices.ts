'use client';
// A finished background job says so, in the transcript, without being asked.
//
// Job results reach the model by riding `wrap_prompt` into the user's NEXT
// message. That is fine for the detail — but it meant a job could finish while
// the user sat waiting for the "I'll report back" the agent had promised, and
// nothing would appear until they spoke first. This renders the fact that the
// news exists the moment it does. It deliberately does NOT speak for the agent:
// no model call, no cost, and nothing that can barge into a running turn.
import { useEffect } from 'react';
import { t } from '@/shared/i18n/t';
import { subscribe } from '@/shared/state/deaconBus';
import { reduceTranscript } from '@/shared/kernel/transcript';

type TranscriptDispatch = React.Dispatch<Parameters<typeof reduceTranscript>[1]>;
type Ref<T> = { current: T };

/** Only `finished` is good news; every other terminal state is reported as the
 * warning it is. Relaying a timed-out or cancelled job as done is the exact
 * laundering `wrap_prompt`'s note text forbids, and the tone must not undo it. */
function toneFor(state: unknown): 'ok' | 'warn' {
  return state === 'finished' ? 'ok' : 'warn';
}

/** Subscribes this chat to `job.finished` for as long as it is mounted. */
export function useJobNotices(dispatch: TranscriptDispatch, aliveRef: Ref<boolean>): void {
  useEffect(() => {
    const s = t().chat.transcript;
    // No session_id on the notification — a background job outlives the turn
    // that started it, so it is a global notice and reaches whichever chat is
    // open, rather than being lost with the session that launched it.
    return subscribe({ method: 'job.finished' }, (event) => {
      if (!aliveRef.current) return;
      const { label, state } = event.params;
      if (typeof label !== 'string' || label === '') return;
      dispatch({
        type: 'notice',
        text: s.jobFinished(label, typeof state === 'string' ? state : 'finished'),
        tone: toneFor(state),
      });
    });
  }, [dispatch, aliveRef]);
}
