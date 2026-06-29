// `regent call` — the live, real-time voice call (LiveKit + the Jarvis web UI).
// Distinct from `regent voice` (turn-based ASR/TTS): this is the duplex call.
import { out, printError } from "@app/cli/runtime.ts";
import { style } from "@shared/ui/style.ts";
import { callServe } from "./callServe.ts";

export function callCommand(profile: string, args: string[]): number | Promise<number> {
  switch (args[0]) {
    case undefined:
    case "serve":
      return callServe(profile);
    default:
      printError("usage: regent call serve");
      out(style.grey("  starts the Jarvis live-call UI + local speech backend. Ctrl-C to stop."));
      return 1;
  }
}
