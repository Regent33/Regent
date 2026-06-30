import { createInterface } from "node:readline/promises";
// `regent code "<task>"` — the coding harness: a read-only PLAN phase, your
// approval, then edit → per-step verify → revert-to-green on failure. `--yes`
// (or `-y`) auto-approves the plan for non-interactive/CI use.
import { out, printError, withClient } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

interface PlanResult {
  session_id: string;
  plan: string;
}

interface StartResult {
  session_id: string;
  report: string;
  verify: { passed: boolean; summary: string } | null;
  reverted: boolean;
}

// Plan/execute run real agent turns — allow them plenty of time.
const HARNESS_TIMEOUT_MS = 600_000;

export async function codeCommand(profile: string, args: readonly string[]): Promise<number> {
  const autoApprove = args.includes("--yes") || args.includes("-y");
  const task = args
    .filter((a) => a !== "--yes" && a !== "-y")
    .join(" ")
    .trim();
  if (!task) {
    printError('usage: regent code "<task>" [--yes]');
    return 1;
  }

  return withClient(profile, async (client: IRpcClient) => {
    // Phase 1 — plan (read-only).
    out(style.grey("Planning (read-only)…"));
    const plan = await client.call<PlanResult>("code.plan", { task }, HARNESS_TIMEOUT_MS);
    if (!plan.ok) {
      printError(plan.error.message);
      return 1;
    }
    out(`\n${style.heading("Plan")}`);
    out(plan.value.plan);

    // Approval gate — non-response is a no (never execute by default).
    if (!autoApprove && !(await confirm("\nApprove this plan and execute?"))) {
      out(style.grey("Aborted — nothing was executed."));
      return 0;
    }

    // Phase 2 — execute → verify → revert-on-fail.
    out(style.grey("\nExecuting…"));
    const res = await client.call<StartResult>(
      "code.start",
      { task, plan: plan.value.plan },
      HARNESS_TIMEOUT_MS,
    );
    if (!res.ok) {
      printError(res.error.message);
      return 1;
    }

    const r = res.value;
    out(`\n${style.heading("Result")}`);
    out(r.report);
    if (r.verify) {
      const tag = r.verify.passed ? style.pass("✓ verify passed") : style.fail("✗ verify failed");
      out(`\n${tag} — ${r.verify.summary}`);
    } else {
      out(style.grey("\n(no verify lane detected — skipped)"));
    }
    if (r.reverted) {
      out(style.warn("↩ reverted to the last green checkpoint"));
    }
    // Non-zero only when a failure was left in place (verify failed and the tree
    // could not be reverted); a reverted failure leaves a clean tree.
    return r.verify && !r.verify.passed && !r.reverted ? 1 : 0;
  });
}

/** Prompt a y/N confirmation on stdin. Anything but y/yes is a no. */
async function confirm(question: string): Promise<boolean> {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    const answer = (await rl.question(`${question} [y/N] `)).trim().toLowerCase();
    return answer === "y" || answer === "yes";
  } finally {
    rl.close();
  }
}
