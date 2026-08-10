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
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { reduceTranscript } from '@/shared/kernel/transcript';

type TranscriptDispatch = React.Dispatch<Parameters<typeof reduceTranscript>[1]>;
type Ref<T> = { current: T };

/** The subset of a `job.list` row this surface reads. `delivered` distinguishes
 * "still running" from "finished, and you have not been told". An older deacon
 * omits it, which reads as undefined and is correctly treated as not-news. */
type JobListRow = {
  readonly id?: string;
  readonly label?: string;
  readonly state?: string;
  readonly delivered?: boolean;
};

/** Only `finished` is good news; every other terminal state is reported as the
 * warning it is. Relaying a timed-out or cancelled job as done is the exact
 * laundering `wrap_prompt`'s note text forbids, and the tone must not undo it. */
function toneFor(state: unknown): 'ok' | 'warn' {
  return state === 'finished' ? 'ok' : 'warn';
}

/** Job ids already announced on this surface.
 *
 * Module scope, not per-mount: the push and the mount replay are two routes to
 * the same news, and a job that finished while the chat was open would
 * otherwise be announced again the moment the user navigated back. This never
 * needs eviction in practice — it holds one short id per finished job for the
 * life of the window — and it is deliberately NOT the durable guard: the ledger
 * already owns that, via `delivered_at`. */
const announced = new Set<string>();

/** Subscribes this chat to `job.finished`, and replays anything that finished
 * while nobody was listening.
 *
 * The push is a best-effort stdio notification rendered into React state, so a
 * reload, a route change, or an app restart dropped it — the job's detail still
 * reached the model on the next turn, but the proactive "it's done" the agent
 * promised was simply lost. `job.list` now also returns finished-but-undelivered
 * work, so the replay reads the ledger instead of trusting a live event to have
 * been seen. It self-terminates: the next successful turn marks those delivered
 * and they stop coming back. */
export function useJobNotices(dispatch: TranscriptDispatch, aliveRef: Ref<boolean>): void {
  useEffect(() => {
    const s = t().chat.transcript;
    const announce = (label: unknown, state: unknown, id: unknown): void => {
      if (!aliveRef.current) return;
      if (typeof label !== 'string' || label === '') return;
      if (typeof id === 'string' && id !== '') {
        if (announced.has(id)) return;
        announced.add(id);
      }
      dispatch({
        type: 'notice',
        text: s.jobFinished(label, typeof state === 'string' ? state : 'finished'),
        tone: toneFor(state),
      });
    };

    void (async () => {
      const listed = await deaconRequest<JobListRow[]>('job.list', {});
      if (!listed.ok || !Array.isArray(listed.value)) return;
      for (const job of listed.value) {
        // Live work is not news; only work that finished unannounced is.
        if (job.delivered !== false || job.state === 'queued' || job.state === 'running') continue;
        announce(job.label, job.state, job.id);
      }
    })();

    // No session_id on the notification — a background job outlives the turn
    // that started it, so it is a global notice and reaches whichever chat is
    // open, rather than being lost with the session that launched it.
    return subscribe({ method: 'job.finished' }, (event) => {
      const { label, state, job_id: jobId } = event.params;
      announce(label, state, jobId);
    });
  }, [dispatch, aliveRef]);
}
