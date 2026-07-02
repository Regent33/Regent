// `bun run link [-- --dir <path>]` - put the compiled `regent` on the PATH.
// Target directory, in order of precedence:
//   1. --dir <path> (or a bare positional path)
//   2. REGENT_LINK_DIR
//   3. default: %USERPROFILE%\.bun\bin (Windows, on PATH via the Bun
//      installer) or ~/.local/bin (macOS/Linux)
// Windows gets a .cmd shim; macOS/Linux a symlink. Both point at dist/, so
// re-running `bun run compile` updates `regent` with no re-link.
import { existsSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const exe = process.platform === "win32" ? "regent-cli.exe" : "regent-cli";
const built = resolve(import.meta.dir, "..", "dist", exe);
if (!existsSync(built)) {
  console.error(`not built yet: ${built}\nrun \`bun run compile\` first`);
  process.exit(1);
}

const args = process.argv.slice(2);
const dirFlag = args.includes("--dir")
  ? args[args.indexOf("--dir") + 1]
  : args.find((a) => !a.startsWith("-"));
const dir = resolve(
  dirFlag ??
    process.env.REGENT_LINK_DIR ??
    (process.platform === "win32"
      ? join(homedir(), ".bun", "bin")
      : join(homedir(), ".local", "bin")),
);
mkdirSync(dir, { recursive: true });

if (process.platform === "win32") {
  const shim = join(dir, "regent.cmd");
  writeFileSync(shim, `@echo off\r\n"${built}" %*\r\n`);
  console.info(`installed shim: ${shim}`);
} else {
  const link = join(dir, "regent");
  rmSync(link, { force: true });
  symlinkSync(built, link);
  console.info(`installed symlink: ${link} -> ${built}`);
}

// The shim points at dist/, so future `bun run compile` runs update `regent`
// automatically - re-linking is only needed if the repo moves.
const onPath = (process.env.PATH ?? "")
  .split(process.platform === "win32" ? ";" : ":")
  .some((p) => p && resolve(p) === resolve(dir));
if (!onPath) {
  console.warn(
    process.platform === "win32"
      ? `note: ${dir} is not on PATH - add it (installing Bun normally does this)`
      : `note: ${dir} is not on PATH - add 'export PATH="$HOME/.local/bin:$PATH"' to your shell profile`,
  );
}
console.info("verify: regent --version && regent doctor");
