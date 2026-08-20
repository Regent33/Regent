import { unattendedMarkers } from "@features/chat/domain/posture.ts";
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
  /** Short markers for anything less guarded than default — usually empty. */
  readonly unattended: readonly string[];
}

const INITIAL: BootstrapState = {
  phase: "connecting",
  error: "",
  model: "—",
  sessionId: "",
  skills: [],
  tools: [],
  unattended: [],
};

export function useBootstrap(client: IRpcClient, resumeId: string | undefined): BootstrapState {
  const [state, setState] = useState<BootstrapState>(INITIAL);

  useEffect(() => {
    let cancelled = false;
    const fail = (message: string) =>
      !cancelled && setState((s) => ({ ...s, phase: "error", error: message }));

    void (async () => {
      // This idempotent health check is the only call the DI wrapper may replay
      // if the selected deacon dies just after its candidate probe.
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
        : await client.call<{ session_id: string }>(
            "session.create",
            // Only claim the card when there is a keyboard to drive it with:
            // a piped run would be handed a question it cannot answer, where
            // the deacon's numbered-text fallback still works.
            process.stdin.isTTY ? { capabilities: ["questions"] } : {},
            30_000,
          );
      if (cancelled) return;
      if (!created.ok) return fail(created.error.message);

      const [model, skills, tools, config] = await Promise.all([
        client.call<{ model: string }>("model.get", {}, 10_000),
        client.call<Array<{ name: string; tags?: string[] }>>("skills.list", {}, 10_000),
        client.call<Array<{ name: string; toolset?: string }>>("tools.list", {}, 10_000),
        // Only for the posture markers on the status line. A failure here must
        // never block the chat, so the markers just come back empty.
        client.call<unknown>("config.get", {}, 10_000),
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
        unattended: unattendedMarkers(config.ok ? config.value : null, process.env),
      });
    })();

    return () => {
      cancelled = true;
    };
  }, [client, resumeId]);

  return state;
}
