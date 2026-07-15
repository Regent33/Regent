import { useCallback, useMemo, useState } from "react";
import type { InstallOptions, Screen, Stage, StageStatus } from "@/app/state";
import { defaultOptions, freshStages } from "@/app/state";
import { BrandHeader } from "@/app/ui/BrandHeader";
import { Welcome } from "@/app/screens/Welcome";
import { License } from "@/app/screens/License";
import { Location } from "@/app/screens/Location";
import { Progress } from "@/app/screens/Progress";
import { Finish } from "@/app/screens/Finish";
import { Failure } from "@/app/screens/Failure";

// Real path is resolved by the Rust backend at Phase 2 (per-user LocalAppData);
// shown as a placeholder until then so the flow is previewable in a browser.
const DEFAULT_DIR = "%LOCALAPPDATA%\\Programs\\Regent";

export function App() {
  const [screen, setScreen] = useState<Screen>("welcome");
  const [options, setOptions] = useState<InstallOptions>(() =>
    defaultOptions(DEFAULT_DIR),
  );
  const [stages, setStages] = useState<Stage[]>(freshStages);
  const [log, setLog] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const patchStage = useCallback((id: string, status: StageStatus) => {
    setStages((prev) => prev.map((s) => (s.id === id ? { ...s, status } : s)));
  }, []);

  const runInstall = useCallback(async () => {
    setScreen("progress");
    setError(null);
    setStages(freshStages());
    setLog([]);
    try {
      // Phase 2 replaces this with invoke("start_install", options) + a
      // subscription to staged progress events from the Rust backend. Until
      // then both dev modes (browser and `tauri dev`) walk the simulation so
      // the whole UI is testable.
      await simulateForPreview(patchStage, setLog);
      setScreen("finish");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setScreen("failure");
    }
  }, [patchStage]);

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
    <div className="flex h-full flex-col bg-bg">
      <header className="flex items-center justify-between px-6 pb-3 pt-5">
        <BrandHeader />
      </header>
      {/* key={screen} remounts on navigation so the fadeIn entrance replays. */}
      <main
        key={screen}
        className="relative flex-1 overflow-hidden px-6 pb-6 motion-safe:animate-[fadeIn_260ms_cubic-bezier(0.23,1,0.32,1)]"
      >
        {body}
      </main>
    </div>
  );
}

// Dev-only: walks the stages so the wizard is clickable in `bun run dev`
// (no Tauri). The real staged install replaces this at Phase 2.
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
