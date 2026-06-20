import type { IRpcClient } from "@shared/kernel/contracts.ts";
// Bootstrap viewmodel: connect → health → open a session → fetch the welcome
// data, exposing a small state machine the App renders. Stale responses are
// ignored on unmount (latest-wins via the `cancelled` guard) so a fast quit
// never writes into a torn-down tree.
import { useEffect, useState } from "react";

export type BootstrapPhase = "connecting" | "ready" | "error";

export interface SkillInfo {
  readonly name: string;
  readonly tags: readonly string[];
}
export interface ToolInfo {
  readonly name: string;
  readonly toolset: string;
}

export interface BootstrapState {
  readonly phase: BootstrapPhase;
  readonly error: string;
  readonly model: string;
  readonly sessionId: string;
  readonly skills: readonly SkillInfo[];
  readonly tools: readonly ToolInfo[];
}

const INITIAL: BootstrapState = {
  phase: "connecting",
  error: "",
  model: "—",
  sessionId: "",
  skills: [],
  tools: [],
};

export function useBootstrap(client: IRpcClient, resumeId: string | undefined): BootstrapState {
  const [state, setState] = useState<BootstrapState>(INITIAL);

  useEffect(() => {
    let cancelled = false;
    const fail = (message: string) =>
      !cancelled && setState((s) => ({ ...s, phase: "error", error: message }));

    void (async () => {
      const health = await client.call("health", {}, 10_000);
      if (cancelled) return;
      if (!health.ok) return fail(health.error.message);

      // Resume an existing session if asked (`sessions resume <id>`), else open a fresh one.
      const created = resumeId
        ? await client.call<{ session_id: string }>(
            "session.resume",
            { session_id: resumeId },
            30_000,
          )
        : await client.call<{ session_id: string }>("session.create", {}, 30_000);
      if (cancelled) return;
      if (!created.ok) return fail(created.error.message);

      const [model, skills, tools] = await Promise.all([
        client.call<{ model: string }>("model.get", {}, 10_000),
        client.call<Array<{ name: string; tags?: string[] }>>("skills.list", {}, 10_000),
        client.call<Array<{ name: string; toolset?: string }>>("tools.list", {}, 10_000),
      ]);
      if (cancelled) return;

      setState({
        phase: "ready",
        error: "",
        sessionId: created.value.session_id,
        model: model.ok ? model.value.model : "—",
        skills:
          skills.ok && Array.isArray(skills.value)
            ? skills.value.map((s) => ({ name: s.name, tags: s.tags ?? [] }))
            : [],
        tools:
          tools.ok && Array.isArray(tools.value)
            ? tools.value.map((t) => ({ name: t.name, toolset: t.toolset ?? "other" }))
            : [],
      });
    })();

    return () => {
      cancelled = true;
    };
  }, [client, resumeId]);

  return state;
}
