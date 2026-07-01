// `regent logs [-f]` — show (and optionally follow) the deacon's newest rolling
// log file under $REGENT_HOME/logs/. Mirrors logs.go.
import {
  closeSync,
  existsSync,
  openSync,
  readFileSync,
  readSync,
  readdirSync,
  statSync,
} from "node:fs";
import { join } from "node:path";
import { parseFlags } from "@app/cli/args.ts";
import { printError } from "@app/cli/runtime.ts";
import { regentHome } from "@shared/infrastructure/deacon/locate.ts";

export async function logsCommand(profile: string, args: string[]): Promise<number> {
  const { values } = parseFlags(args, { follow: { type: "boolean", alias: "f" } });
  const dir = join(regentHome(profile), "logs");
  const noLogs = `no log files in ${dir} (has the deacon run yet?)`;

  if (!existsSync(dir)) {
    printError(noLogs);
    return 1;
  }
  const latest = readdirSync(dir)
    .filter((f) => f.startsWith("regent.log"))
    .sort()
    .at(-1);
  if (!latest) {
    printError(noLogs);
    return 1;
  }

  const path = join(dir, latest);
  process.stdout.write(readFileSync(path, "utf8"));
  if (!values.follow) return 0;
  return await follow(path);
}

// Poll for appended bytes (newest rolling file) until Ctrl-C.
function follow(path: string): Promise<number> {
  return new Promise((resolve) => {
    let pos = statSync(path).size;
    const id = setInterval(() => {
      const size = statSync(path).size;
      if (size <= pos) return;
      const fd = openSync(path, "r");
      const buf = Buffer.alloc(size - pos);
      readSync(fd, buf, 0, buf.length, pos);
      closeSync(fd);
      process.stdout.write(buf);
      pos = size;
    }, 500);
    process.on("SIGINT", () => {
      clearInterval(id);
      resolve(0);
    });
  });
}
