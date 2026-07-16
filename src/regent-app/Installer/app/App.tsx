import { useCallback, useEffect, useMemo, useState } from "react";
import type { InstallOptions, Mode, Screen, Stage, StageStatus } from "@/app/state";
import { defaultOptions, freshStages } from "@/app/state";
import { Welcome } from "@/app/screens/Welcome";
import { License } from "@/app/screens/License";
import { Location } from "@/app/screens/Location";
import { Confirm } from "@/app/screens/Confirm";
import { Progress } from "@/app/screens/Progress";
import { Finish } from "@/app/screens/Finish";
import { Removed } from "@/app/screens/Removed";
import { Failure } from "@/app/screens/Failure";

// Placeholder shown until the Rust backend reports the real per-user path
// (browser preview has no backend, so it keeps this).
const DEFAULT_DIR = "%LOCALAPPDATA%\\Programs\\Regent";
const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
// Uninstall preview: `bun run dev` has no backend to ask, so the flow is
// reachable at /?uninstall for UI work — and /?existing previews the
// "already installed" offer on the welcome screen.
const previewMode: Mode =
  typeof window !== "undefined" &&
  window.location.search.includes("uninstall")
    ? "uninstall"
    : "install";
const previewExisting: string | null =
  typeof window !== "undefined" &&
  window.location.search.includes("existing")
    ? "~/.local/share/Regent"
    : null;

type InstallEventPayload =
  | { type: "stage"; id: string; status: StageStatus }
  | { type: "log"; line: string }
  | { type: "done" }
  | { type: "failed"; error: string };

export function App() {
  const [mode, setMode] = useState<Mode>(previewMode);
  const [screen, setScreen] = useState<Screen>(
    previewMode === "uninstall" ? "confirm" : "welcome",
  );
  const [options, setOptions] = useState<InstallOptions>(() =>
    defaultOptions(DEFAULT_DIR),
  );
  const [stages, setStages] = useState<Stage[]>(() => freshStages(previewMode));
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  // A Regent already on this machine (macOS/Linux — Windows uses Apps &
  // features). The welcome screen offers to remove it instead.
  const [existing, setExisting] = useState<string | null>(previewExisting);
  // Whether the BINARY was launched as an uninstaller (uninstall.exe), as
  // opposed to the user flipping into the uninstall flow from the welcome
  // screen. Decides what Cancel on the confirm screen means: quit the app
  // (there is nothing else an uninstaller could show) vs. back to welcome.
  const [nativeUninstall, setNativeUninstall] = useState(
    previewMode === "uninstall",
  );

  // Which flow are we, and where. The backend routes on its own exe name, so
  // this is the first thing the UI has to ask.
  useEffect(() => {
    if (!isTauri) return;
    void import("@tauri-apps/api/core").then(({ invoke }) =>
      invoke<{ mode: Mode; installDir: string; existingInstall: string | null }>(
        "startup",
      )
        .then((s) => {
          setMode(s.mode);
          setStages(freshStages(s.mode));
          if (s.installDir) {
            setOptions((o) => ({ ...o, installDir: s.installDir }));
          }
          setExisting(s.existingInstall ?? null);
          if (s.mode === "uninstall") {
            setNativeUninstall(true);
            setScreen("confirm");
          }
        })
        .catch(() => {}),
    );
  }, []);

  // "Remove it instead" on the welcome screen: flip the whole app into the
  // uninstall flow — from here on it is exactly the flow uninstall.exe runs.
  const uninstallInstead = useCallback(() => {
    if (!existing) return;
    setMode("uninstall");
    setStages(freshStages("uninstall"));
    setOptions((o) => ({ ...o, installDir: existing }));
    setScreen("confirm");
  }, [existing]);

  const patchStage = useCallback((id: string, status: StageStatus) => {
    setStages((prev) => prev.map((s) => (s.id === id ? { ...s, status } : s)));
  }, []);

  // Both flows stream the same event shape over the same channel and differ
  // only in the command and the screen they land on, so they share one runner.
  const run = useCallback(async () => {
    const uninstalling = mode === "uninstall";
    const doneScreen: Screen = uninstalling ? "removed" : "finish";
    setScreen("progress");
    setError(null);
    setStages(freshStages(mode));
    setLog([]);

    if (!isTauri) {
      // Browser dev preview — no native backend; walk the simulation.
      try {
        await simulateForPreview(mode, patchStage, setLog);
        setScreen(doneScreen);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setScreen("failure");
      }
      return;
    }

    // Native path: subscribe to staged progress, then kick the work off.
    const { invoke } = await import("@tauri-apps/api/core");
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<InstallEventPayload>("install-event", (ev) => {
      const p = ev.payload;
      if (p.type === "stage") patchStage(p.id, p.status);
      else if (p.type === "log") setLog((l) => [...l, p.line]);
      else if (p.type === "done") {
        void unlisten();
        setScreen(doneScreen);
      } else if (p.type === "failed") {
        void unlisten();
        setError(p.error);
        setScreen("failure");
      }
    });
    try {
      await invoke(
        uninstalling ? "start_uninstall" : "start_install",
        uninstalling ? {} : { options },
      );
    } catch (e) {
      void unlisten();
      setError(e instanceof Error ? e.message : String(e));
      setScreen("failure");
    }
  }, [mode, options, patchStage]);

  const close = useCallback(async () => {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("quit");
    } catch {
      /* not running under Tauri */
    }
  }, []);

  const body = useMemo(() => {
    switch (screen) {
      case "welcome":
        return (
          <Welcome
            onNext={() => setScreen("license")}
            existingInstall={existing}
            onUninstall={uninstallInstead}
          />
        );
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
            onInstall={run}
          />
        );
      case "confirm":
        return (
          <Confirm
            installDir={options.installDir}
            onCancel={
              nativeUninstall
                ? close
                : () => {
                    // Arrived via "remove it instead" — undo the flip.
                    setMode("install");
                    setStages(freshStages("install"));
                    setScreen("welcome");
                  }
            }
            onUninstall={run}
          />
        );
      case "progress":
        return (
          <Progress
            stages={stages}
            log={log}
            title={mode === "uninstall" ? "Uninstalling…" : "Installing…"}
          />
        );
      case "finish":
        return <Finish options={options} />;
      case "removed":
        return <Removed onClose={close} />;
      case "failure":
        return (
          <Failure
            error={error}
            log={log}
            onRetry={run}
            onBack={() => setScreen(mode === "uninstall" ? "confirm" : "location")}
          />
        );
    }
  }, [screen, mode, options, stages, log, error, run, close, existing, uninstallInstead, nativeUninstall]);

  return (
    // No in-window header — the OS title bar reads "Regent Setup". key={screen}
    // remounts on navigation so the fadeIn entrance replays per screen.
    <div className="h-full bg-bg">
      <main
        key={screen}
        className="h-full overflow-hidden px-8 py-10 animate-[fadeIn_260ms_cubic-bezier(0.23,1,0.32,1)]"
      >
        {body}
      </main>
    </div>
  );
}

// Dev-only: walks the stages so the wizard is clickable in `bun run dev`
// (no Tauri). The native path uses the Rust backend's install-event stream.
async function simulateForPreview(
  mode: Mode,
  patch: (id: string, s: StageStatus) => void,
  setLog: (fn: (p: string[]) => string[]) => void,
) {
  // Same order the backend uses, so the preview reads like the real thing.
  const order =
    mode === "uninstall" ? ["app", "core", "wire"] : ["core", "app", "wire"];
  for (const id of order) {
    patch(id, "running");
    setLog((p) => [...p, `[preview] ${id}: working…`]);
    await new Promise((r) => setTimeout(r, 700));
    patch(id, "done");
  }
}
