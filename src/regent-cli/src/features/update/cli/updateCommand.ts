// `regent update` is status-only in Phase 0: it never downloads or replaces
// installed binaries/configuration. It may start the deacon, whose bounded,
// redirect-filtered background checker owns and refreshes the cached verdict.
// A flat binary pair cannot be replaced atomically; claiming apply before the
// versioned launcher + journal design lands would make this command dangerous.
import { CLI_VERSION } from "@app/cli/help.ts";
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";
import { fetchUpdateStatus } from "../data/checkForUpdate.ts";
import { RELEASES_URL } from "../domain/notice.ts";

export interface UpdateOutput {
  readonly line: (message: string) => void;
  readonly error: (message: string) => void;
}

const defaultOutput: UpdateOutput = { line: out, error: printError };

export async function updateCommand(
  client: IRpcClient,
  args: readonly string[],
  output: UpdateOutput = defaultOutput,
): Promise<number> {
  if (args.some((arg) => arg !== "--check")) {
    output.error("usage: regent update [--check]");
    return 2;
  }
  const status = await fetchUpdateStatus(client, 5_000);
  if (!status) {
    output.error(
      "update status is unavailable from this deacon; run `regent doctor` and try again",
    );
    return 1;
  }

  output.line(style.heading("Regent update status"));
  output.line(`  CLI       ${CLI_VERSION}`);
  output.line(`  deacon    ${status.current || "unknown"}`);
  output.line(`  latest    ${status.latest ?? "not checked yet"}`);
  const source = status.source ?? "unknown";
  output.line(`  source    ${source}`);
  if (typeof status.checkedAt === "number") {
    output.line(`  checked   ${new Date(status.checkedAt * 1_000).toLocaleString()}`);
  }
  const mixedInstall = status.current !== CLI_VERSION;
  if (mixedInstall) {
    output.line(
      style.warn(`⚠ mixed installation: CLI ${CLI_VERSION}, deacon ${status.current || "unknown"}`),
    );
  }
  if (status.note) output.line(style.warn(`⚠ last check: ${status.note}`));

  if (source === "disabled") {
    output.error("background update checks are disabled by REGENT_NO_UPDATE_CHECK");
    return 1;
  }
  if (status.latest === null || source === "never") {
    output.error(
      "this Regent home has no cached release check yet; keep the deacon running, then retry",
    );
    return 1;
  }
  if (mixedInstall) {
    output.error(
      `the CLI and deacon are different releases; reinstall one complete Regent release from ${RELEASES_URL}`,
    );
    return 1;
  }
  if (status.available) {
    output.line(`\nRegent ${status.latest} is available: ${RELEASES_URL}`);
    output.line(
      style.grey(
        "Use the verified installer for your platform; in-place CLI apply is not shipped yet.",
      ),
    );
    return 0;
  }
  output.line(`\n${style.pass("✓")} Regent ${CLI_VERSION} is up to date.`);
  return 0;
}
