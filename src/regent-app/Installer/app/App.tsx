import { useCallback, useEffect, useMemo, useState } from "react";
import type { InstallOptions, Screen, Stage, StageStatus } from "@/app/state";
import { defaultOptions, freshStages } from "@/app/state";
import { Welcome } from "@/app/screens/Welcome";
import { License } from "@/app/screens/License";
import { Location } from "@/app/screens/Location";
import { Progress } from "@/app/screens/Progress";
import { Finish } from "@/app/screens/Finish";
import { Failure } from "@/app/screens/Failure";

// Placeholder shown until the Rust backend reports the real per-user path
// (browser preview has no backend, so it keeps this).
const DEFAULT_DIR = "%LOCALAPPDATA%\\Programs\\Regent";
const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type InstallEventPayload =
  | { type: "stage"; id: string; status: StageStatus }
  | { type: "log"; line: string }
  | { type: "done" }
  | { type: "failed"; error: string };

export function App() {
  const [screen, setScreen] = useState<Screen>("welcome");
  const [options, setOptions] = useState<InstallOptions>(() =>
    defaultOptions(DEFAULT_DIR),
  );
  const [stages, setStages] = useState<Stage[]>(freshStages);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  // In the native app, ask the backend for the real per-user default dir.
  useEffect(() => {
    if (!isTauri) return;
    void import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke<string>("default_install_dir")
        .then((dir) => setOptions((o) => ({ ...o, installDir: dir })))
        .catch(() => {}),
    );
  }, []);

  const patchStage = useCallback((id: string, status: StageStatus) => {
    setStages((prev) => prev.map((s) => (s.id === id ? { ...s, status } : s)));
  }, []);

  const runInstall = useCallback(async () => {
    setScreen("progress");
    setError(null);
    setStages(freshStages());
    setLog([]);

    if (!isTauri) {
      // Browser dev preview — no native backend; walk the simulation.
      try {
        await simulateForPreview(patchStage, setLog);
        setScreen("finish");
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setScreen("failure");
      }
      return;
    }

    // Native path: subscribe to staged progress, then kick off the install.
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<InstallEventPayload>("install-event", (ev) => {
      const p = ev.payload;
      if (p.type === "stage") patchStage(p.id, p.status);
      else if (p.type === "log") setLog((l) => [...l, p.line]);
      else if (p.type === "done") {
        void unlisten();
        setScreen("finish");
      } else if (p.type === "failed") {
        void unlisten();
        setError(p.error);
        setScreen("failure");
      }
    });
    try {
      await invoke("start_install", { options });
    } catch (e) {
      void unlisten();
      setError(e instanceof Error ? e.message : String(e));
      setScreen("failure");
    }
  }, [options, patchStage]);

  const body = useMemo(() => {
    switch (screen) {
      case "welcome":
        return <Welcome onNext={() => setScreen("license")} />;
      case "license":
        return (
          <License
            onBack={() => setScreen("welcome")}
            onNext={() => setScreen("location")}
          />
        );
      case "location":
        return (
          <Location
            options={options}
            onChange={setOptions}
            onBack={() => setScreen("license")}
            onInstall={runInstall}
          />
        );
      case "progress":
        return <Progress stages={stages} log={log} />;
      case "finish":
        return <Finish options={options} />;
      case "failure":
        return (
          <Failure
            error={error}
            log={log}
            onRetry={runInstall}
            onBack={() => setScreen("location")}
          />
        );
    }
  }, [screen, options, stages, log, error, runInstall]);

  return (
    // No in-window header — the OS title bar reads "Regent Setup". key={screen}
    // remounts on navigation so the fadeIn entrance replays per screen.
    <div className="h-full bg-bg">
      <main
        key={screen}
        className="h-full overflow-hidden px-8 py-10 motion-safe:animate-[fadeIn_260ms_cubic-bezier(0.23,1,0.32,1)]"
      >
        {body}
      </main>
    </div>
  );
}

// Dev-only: walks the stages so the wizard is clickable in `bun run dev`
// (no Tauri). The native path uses the Rust backend's install-event stream.
async function simulateForPreview(
  patch: (id: string, s: StageStatus) => void,
  setLog: (fn: (p: string[]) => string[]) => void,
) {
  for (const id of ["core", "app", "wire"]) {
    patch(id, "running");
    setLog((p) => [...p, `[preview] ${id}: working…`]);
    await new Promise((r) => setTimeout(r, 700));
    patch(id, "done");
  }
}
