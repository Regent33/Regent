// CLI runtime helpers shared by the one-shot subcommands.
import { buildContainer } from "@app/di/container.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

/** Print a line to stdout. */
export const out = (s: string): void => {
  process.stdout.write(`${s}\n`);
};

/** Print an error to stderr. */
export function printError(message: string): void {
  process.stderr.write(`${style.fail("✗")} ${message}\n`);
}

/**
 * Spawn the daemon, health-check it, run `fn`, and always close. The Go CLI's
 * `withClient` pattern: subcommands never manage the connection themselves.
 */
export async function withClient(
  profile: string,
  fn: (client: IRpcClient) => Promise<number>,
): Promise<number> {
  const deps = buildContainer(profile);
  if (!deps.ok) {
    printError(deps.error.message);
    return 1;
  }
  const { client } = deps.value;
  const health = await client.call("health", {}, 10_000);
  if (!health.ok) {
    printError(`daemon health check failed: ${health.error.message}`);
    await client.close();
    return 1;
  }
  try {
    return await fn(client);
  } finally {
    await client.close();
  }
}
