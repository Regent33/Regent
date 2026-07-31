// `regent ask` — one question, one answer, exit (plan Phase E).
//
// The whole front door is two forms:
//   regent ask "what changed in this repo today"
//   echo "summarise this" | regent ask
//
// Everything else is behind flags most people never type. The machine contract
// (exit codes, stream discipline, the terminal event) is in ../domain/askRun.ts
// so it can be tested without a provider.
//
// Approvals: without `--yes`, every action that REACHES THE APPROVAL GATE is
// denied and the run continues. That is all it means. It is not a read-only
// mode, not a dry run and not a sandbox: file writes never reach the gate at all
// (docs/audits/approval-sandbox-boundary-2026-07-31.md §2).
import { parseFlags, unknownFlags } from "@app/cli/args.ts";
import { err, out, printError } from "@app/cli/runtime.ts";
import { buildContainer } from "@app/di/container.ts";
import {
  ASK_EXIT,
  ASK_INITIAL,
  type AskState,
  askExitCode,
  reduceAsk,
  resolveApproval,
  terminalEvent,
} from "@features/ask/domain/askRun.ts";

const SPEC = {
  json: { type: "boolean" },
  events: { type: "boolean" },
  yes: { type: "boolean" },
  timeout: { type: "string" },
  session: { type: "string" },
  continue: { type: "boolean", alias: "c" },
} as const;

/** Read all of stdin when it is a pipe. Never waits on an interactive terminal. */
async function readStdin(): Promise<string> {
  if (process.stdin.isTTY === true) return "";
  const chunks: Uint8Array[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk as Uint8Array);
  return Buffer.concat(chunks).toString("utf8");
}

export async function askCommand(profile: string, args: string[]): Promise<number> {
  const bad = unknownFlags(args, SPEC);
  if (bad.length > 0) {
    printError(`unknown option: ${bad.join(" ")}   (regent ask --help)`);
    return ASK_EXIT.usage;
  }
  const { values, positionals } = parseFlags(args, SPEC);
  const piped = await readStdin();
  const positional = positionals.join(" ").trim();

  // Both at once is a usage error with a message that says so — clearer than a
  // --stdin flag nobody would need to learn.
  if (positional !== "" && piped.trim() !== "") {
    printError("give a prompt OR pipe one in, not both");
    return ASK_EXIT.usage;
  }
  const prompt = positional !== "" ? positional : piped.trim();
  if (prompt === "") {
    printError('usage: regent ask "<question>"   (or pipe the question in)');
    return ASK_EXIT.usage;
  }

  const json = values.json === true;
  const events = values.events === true;
  if (json && events) {
    printError("--json and --events are different output modes; pick one");
    return ASK_EXIT.usage;
  }
  const timeoutMs = (() => {
    const raw = typeof values.timeout === "string" ? Number(values.timeout) : Number.NaN;
    return Number.isFinite(raw) && raw > 0 ? raw * 1000 : 0;
  })();
  if (typeof values.timeout === "string" && timeoutMs === 0) {
    printError("--timeout takes a positive number of seconds");
    return ASK_EXIT.usage;
  }

  // Progress and diagnostics go to stderr; the answer owns stdout. Under
  // --events stdout carries NDJSON only, and stderr is reserved for failures
  // before the stream starts.
  const emit = (obj: Record<string, unknown>): void => {
    if (events) out(JSON.stringify(obj));
  };

  const deps = await buildContainer(profile);
  if (!deps.ok) {
    printError(deps.error.message);
    return ASK_EXIT.unavailable;
  }
  const { client } = deps.value;
  let state: AskState = ASK_INITIAL;
  let sessionId = "";
  // A run is not a session: `-c` and `--session` deliberately reuse one, so
  // reporting the session id as the run id would give several runs the same
  // "unique" id. The deacon has no per-turn id yet (tracked), so this identifies
  // the INVOCATION and nothing more.
  const runId = `run_${crypto.randomUUID().replaceAll("-", "").slice(0, 16)}`;

  try {
    const health = await client.call("health", {}, 15_000);
    if (!health.ok) {
      printError(`deacon health check failed: ${health.error.message}`);
      return ASK_EXIT.unavailable;
    }
    // `-c` is the common "keep going" case; `--session <id>` is the scripted
    // one. Both resume; the difference is only how the id is found.
    let resume = typeof values.session === "string" ? values.session : "";
    if (resume === "" && values.continue === true) {
      const recent = await client.call<Array<{ session_id: string }>>(
        "session.list",
        { limit: 1 },
        15_000,
      );
      if (!recent.ok || recent.value.length === 0) {
        printError("no session to continue — run `regent ask` once first");
        return ASK_EXIT.unavailable;
      }
      resume = recent.value[0]?.session_id ?? "";
    }
    const created = resume
      ? await client.call<{ session_id: string }>("session.resume", { session_id: resume }, 30_000)
      : await client.call<{ session_id: string }>("session.create", {}, 30_000);
    if (!created.ok) {
      printError(created.error.message);
      return ASK_EXIT.unavailable;
    }
    sessionId = created.value.session_id;
    // The session id never goes to answer stdout — a script reading the answer
    // must not have to strip it back out.
    if (events)
      emit({ type: "run.started", schema_version: 1, run_id: runId, session_id: sessionId });
    else if (!json) err(`session ${sessionId}`);

    const finished = new Promise<void>((resolve) => {
      let printed = 0;
      client.onNotification(({ method, params }) => {
        const before = state;
        state = reduceAsk(state, method, params ?? {});
        if (events && method !== "message.delta") {
          // Canonical fields go LAST: a daemon param named `type` or
          // `schema_version` must not be able to overwrite the envelope.
          emit({ ...(params ?? {}), type: method, schema_version: 1, run_id: runId });
        }
        // Stream the answer as it arrives, in the default mode only: --json is
        // one document by construction and --events owns stdout.
        if (!json && !events && state.answer.length > printed) {
          process.stdout.write(state.answer.slice(printed));
          printed = state.answer.length;
        }
        // A headless run must never prompt. An approval is answered by policy
        // and the run continues; the denial is reported, not swallowed.
        if (state.pendingApproval !== null && before.pendingApproval === null) {
          const approved = values.yes === true;
          const { tool, action } = state.pendingApproval;
          state = resolveApproval(state, approved);
          if (events) {
            emit({
              tool,
              action,
              approved,
              type: "approval.decided",
              schema_version: 1,
              run_id: runId,
            });
          } else if (!approved) {
            err(`denied: ${tool} (${action}) — pass --yes to allow it`);
          }
          // NOT fire-and-forget: if this never lands, the deacon stays blocked
          // on the approval and the run waits forever for a completion that
          // cannot come. Failing it here is the only way out.
          void client
            .call("approval.respond", { session_id: sessionId, approved }, 15_000)
            .then((r) => {
              if (!r.ok) {
                state = { ...state, done: true, status: "failed" };
                printError(`could not answer the approval: ${r.error.message}`);
                resolve();
              }
            });
        }
        if (state.done) resolve();
      });
    });

    async function waitForRun(): Promise<void> {
      if (timeoutMs > 0) {
        // The loser of this race must be cleared, or a completed run keeps the
        // process alive until a long --timeout elapses.
        let timer: ReturnType<typeof setTimeout> | undefined;
        const timedOut = await Promise.race([
          finished.then(() => false),
          new Promise<boolean>((r) => {
            timer = setTimeout(() => r(true), timeoutMs);
          }),
        ]).finally(() => {
          if (timer !== undefined) clearTimeout(timer);
        });
        if (timedOut) {
          // Cancellation is scoped to this run's session; nothing else is touched.
          await client.call("turn.interrupt", { session_id: sessionId }, 10_000);
          state = { ...state, status: "timed_out" };
        }
      } else {
        await finished;
      }
    }

    const submitted = await client.call(
      "prompt.submit",
      { session_id: sessionId, text: prompt },
      30_000,
    );
    if (submitted.ok) {
      await waitForRun();
    } else {
      // NOT an early return. The event stream has already started, and a
      // consumer must be able to tell a finished-but-failed stream from a
      // process that was cut off mid-write — so this falls through to the
      // single terminal event below.
      printError(submitted.error.message);
      state = { ...state, done: true, status: "failed" };
    }
  } finally {
    await client.close();
  }

  if (json) {
    out(
      JSON.stringify(
        { ...terminalEvent(state, { runId, sessionId }), answer: state.answer },
        null,
        2,
      ),
    );
  } else if (events) {
    emit(terminalEvent(state, { runId, sessionId }));
  } else {
    // The streaming default can leave a PARTIAL answer on stdout after a
    // failure; scripts that need all-or-nothing use --json.
    if (!state.answer.endsWith("\n")) process.stdout.write("\n");
    if (state.status !== "completed") err(`run ${state.status}`);
  }
  return askExitCode(state);
}
