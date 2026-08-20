// `regent mcp serve` — exec the MCP server over inherited stdio so an MCP
// client that spawns this command talks straight to it.
//
// The server is `regent-deacon mcp`, not a separate `regent-mcp` binary: that
// binary was never packaged by CI or either installer, so this command failed
// with "regent-mcp not found" on every machine that installed rather than built
// from source. It lives in the deacon crate already, so the subcommand costs
// nothing to ship.
import { spawn } from "node:child_process";
import { printError } from "@app/cli/runtime.ts";
import { locateBinary, regentHome } from "@shared/infrastructure/deacon/locate.ts";

export function mcpCommand(profile: string, args: string[]): Promise<number> {
  if (args[0] !== "serve") {
    printError("usage: regent mcp serve");
    return Promise.resolve(1);
  }
  const located = locateBinary("regent-deacon", "REGENT_DEACON_PATH");
  if (!located.ok) {
    printError(located.error.message);
    return Promise.resolve(1);
  }
  const home = regentHome(profile);
  return new Promise<number>((resolve) => {
    const child = spawn(located.value, ["mcp"], {
      stdio: "inherit",
      env: { ...process.env, REGENT_HOME: home },
    });
    child.on("error", (e) => {
      printError(e.message);
      resolve(1);
    });
    child.on("exit", (code) => resolve(code ?? 0));
  });
}
