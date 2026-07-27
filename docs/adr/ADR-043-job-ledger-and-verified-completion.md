# ADR-043 — The job ledger, and what "done" is allowed to mean

Date: 2026-07-27 · Status: accepted · Implements W1 of the superiority plan.

## Context

Work that outlives its turn — background tasks, cron, coding, delegation — was
tracked in a process-global `static Mutex<Vec<Task>>`. Two consequences, both
observed:

- A deacon restart dropped every running job without a word, after the user had
  been told "I'll report back". The report simply never came.
- A job's outcome was `Done(String)` or `Failed(String)`. Any text a background
  agent returned became `Done`, including text describing its own failure. One
  archived skill records five recurrences of a single job that each ended "I'll
  report back" with nothing delivered.

"Regent cannot tell when a job is done" was one of the three findings behind
four separate user reports.

## Decision

A durable `jobs` ledger (schema v10, additive: `jobs`, `job_attempts`,
`job_artifacts`).

**Completion is four separate facts**, each `yes`/`no`/`unknown`: process
completed · artifact produced · result validated · outcome achieved. The
terminal state is **derived** from them, not asserted by the caller:
`outcome_achieved = yes` → `Succeeded`, `no` → `Failed`, `unknown` →
`Inconclusive`. `Inconclusive` is a legal terminal state — "it ran and we cannot
tell whether it worked" is an honest answer the old pair could not express.

`Interrupted` is separate again: a job cut off by a restart is neither success
nor failure, is retryable, and is reported to the user as unfinished with an
offer to start it over. Boot recovery marks every job the previous process left
running.

Idempotency is a **partial unique index** over `(idempotency_key)` where state
is live — so a re-fired `background_task` or an overlapping cron tick collapses
onto the running job instead of creating a twin. Row ids are *not* derived from
that key: the key is released on a terminal state, so a key-derived id collided
on the primary key and wedged a schedule after its first failure.

Cancellation and a per-job deadline are enforced in the runner via `select!`;
cron joins the same ledger through a decorator at the composition root rather
than growing a second one (this is the plan's W7).

`Interrupted`, `Cancelled` and `TimedOut` are set by the *runtime*, not derived:
they are facts about the process. `JobLedger::stop` takes a `StopReason` rather
than a free `JobState` precisely so no caller can stamp `Succeeded` on a job
nobody let finish.

Completion is **fenced on the attempt number**. A worker may only close the
attempt it opened, and only while the job is still running — otherwise a process
declared interrupted at an earlier boot could return late and overwrite a newer
attempt's outcome. `max_attempts` is enforced in the same conditional UPDATE.
The DB carries `CHECK` constraints on every state and fact value, so a bad write
surfaces instead of being parsed back as `queued`/`unknown`.

## Consequences

**No path reaches `Succeeded` today.** That is deliberate, and it is the whole
point of the change: nothing in the system validates a job's output, so
`result_validated` is `unknown` everywhere and `outcome_achieved` stays
`unknown` too. Every background and cron job therefore lands on `Inconclusive`
and is reported as "NO VERIFIED RESULT", with the agent's own account preserved
verbatim as a claim the user can read rather than a fact the system asserts.

A first draft did infer success — from "an artifact exists and the report is
non-empty" for background jobs, and from "the reply is non-empty" for cron. A
read-only co-audit called that the original defect wearing a new name, and it
was right: an artifact can be partial or irrelevant, and "I couldn't reach the
API" is a non-empty reply. Both were removed. A test asserts no unvalidated run
can reach `Succeeded`, so wiring a real validator in later has to revisit this
decision explicitly.

Expect visibly more hedging than before. It is accurate hedging.

## Known open (from the co-audit, not fixed here)

- **Delivery is global and unscoped.** There is no requester or conversation on
  a job, so `updates()` delivers every finished job to the next turn on any
  session. Safe only while Regent is single-user; a job started from a chat
  platform would surface on the desktop. Needs an origin scope.
- **Idempotency keys are `kind:label`.** Generic labels collide across sessions,
  and the default label is the task's first 60 chars. Needs a caller-supplied
  token or a hash of the full task plus origin.
- **No worker lease.** Boot recovery interrupts *all* running jobs; correct only
  because the deacon is the sole process that constructs a `SessionManager`, and
  a second deacon already deadlocks on the shared store. Undocumented invariant
  now written down here. Attempt fencing limits the damage if it is ever broken.
- **A store error during terminalization strands a job in `running`** until the
  next boot recovery. No reaper.
- **Artifacts attach to the job, not the attempt**, so a retry sees artifacts
  from the interrupted attempt before it.
- Dropping the work future on a deadline stops polling; it does not prove any
  spawned child process was killed.
