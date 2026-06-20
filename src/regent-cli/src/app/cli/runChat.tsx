import { out, printError } from "@app/cli/runtime.ts";
import { buildContainer } from "@app/di/container.ts";
import { App } from "@app/presentation/App.tsx";
import { logger } from "@shared/infrastructure/logger/logger.ts";
// The interactive chat path: build the container (locate + spawn the daemon),
// render the Ink app, and close the transport on exit. Bare `regent` and
// `regent chat` route here.
import { render } from "ink";

export async function runChat(profile: string, resumeSessionId?: string): Promise<number> {
  const deps = buildContainer(profile);
  if (!deps.ok) {
    logger.error({ operation: "bootstrap", outcome: "failure", message: deps.error.message });
    printError(deps.error.message);
    return 1;
  }
  logger.info({ operation: "bootstrap", outcome: "success", profile: profile || "(default)" });
  // In dev (`bun run dev`), Bun echoes the script command (`$ bun run …`) above
  // our UI; clear it so the CLI opens clean like the compiled binary does.
  // \x1b[3J clears scrollback (needed by VS Code's xterm), \x1b[2J the viewport,
  // \x1b[H homes the cursor. The compiled build sets DEV="false" and skips this.
  if (process.env.DEV !== "false") process.stdout.write("\x1b[3J\x1b[2J\x1b[H");
  const { client } = deps.value;
  // exitOnCtrlC:false — the chat owns Ctrl-C (interrupt, then double-tap to
  // exit). Without this, Ink quits on the first press before our handler runs.
  const app = render(<App client={client} resumeSessionId={resumeSessionId} />, {
    exitOnCtrlC: false,
  });
  await app.waitUntilExit();
  await client.close();
  out(""); // newline after the alt-region tears down
  return 0;
}
