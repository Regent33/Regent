// `regent mcp serve` — exec the regent-mcp server over inherited stdio so an
// MCP client that spawns this command talks straight to it. Mirrors mcp.go.
import { spawn } from "node:child_process";
import { printError } from "@app/cli/runtime.ts";
import { locateBinary, regentHome } from "@shared/infrastructure/deacon/locate.ts";

export function mcpCommand(profile: string, args: string[]): Promise<number> {
  if (args[0] !== "serve") {
    printError("usage: regent mcp serve");
    return Promise.resolve(1);
  }
  const located = locateBinary("regent-mcp", "REGENT_MCP_PATH");
  if (!located.ok) {
    printError(located.error.message);
    return Promise.resolve(1);
  }
  const home = regentHome(profile);
  return new Promise<number>((resolve) => {
    const child = spawn(located.value, [], {
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
