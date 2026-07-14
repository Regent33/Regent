// The interactive first-run wizard: staged arrow-key pickers (provider →
// model → key → confirm) with type-to-filter and Esc-to-go-back, modelled on
// the chat TUI's overlay pickers so onboarding looks like the rest of Regent.
// Base URL is deliberately NOT asked — every provider's default endpoint is
// encoded in the deacon (`regent setup --base-url` covers the rare override).
import { Box, Text, useApp, useInput } from "ink";
import { useState } from "react";
import { KING_ART } from "@shared/ui/brand/kingArt.generated.ts";
import { PixelArt } from "@shared/ui/brand/PixelArt.tsx";
import { palette } from "@shared/ui/tokens/theme.ts";
import type { ProviderInfo } from "../domain/catalog.ts";
import { SelectList } from "./SelectList.tsx";

export interface WizardResult {
  readonly provider: string;
  readonly model: string;
  readonly key: string;
}

interface SetupWizardProps {
  readonly catalog: readonly ProviderInfo[];
  readonly onDone: (result: WizardResult | null) => void;
}

type Stage = "provider" | "model" | "key" | "confirm";

export function SetupWizard({ catalog, onDone }: SetupWizardProps) {
  const { exit } = useApp();
  const [stage, setStage] = useState<Stage>("provider");
  const [idx, setIdx] = useState(0);
  const [filter, setFilter] = useState("");
  const [provider, setProvider] = useState<ProviderInfo | null>(null);
  const [model, setModel] = useState("");
  const [key, setKey] = useState("");

  const finish = (result: WizardResult | null) => {
    onDone(result);
    exit();
  };

  const providerRows = catalog
    .filter((p) => p.name.includes(filter.toLowerCase()))
    .map((p) => ({ label: p.name, hint: p.needs_key ? p.host : `${p.host} · no key needed`, p }));
  const modelRows = (provider?.models ?? [])
    .filter((m) => m.toLowerCase().includes(filter.toLowerCase()))
    .map((m) => ({ label: m }));

  useInput((input, keyev) => {
    const rows = stage === "provider" ? providerRows.length : modelRows.length;
    if (keyev.upArrow) return setIdx((i) => Math.max(0, i - 1));
    if (keyev.downArrow) return setIdx((i) => Math.min(Math.max(0, rows - 1), i + 1));
    if (keyev.escape) {
      if (stage === "provider") return finish(null);
      setFilter("");
      setIdx(0);
      if (stage === "model") setStage("provider");
      else if (stage === "key") setStage("model");
      else setStage(provider?.needs_key ? "key" : "model");
      return;
    }
    if (keyev.return) {
      if (stage === "provider") {
        const row = providerRows[idx];
        if (!row) return;
        setProvider(row.p);
        setFilter("");
        setIdx(0);
        setStage("model");
      } else if (stage === "model") {
        // A filtered pick — or, with no matching row, the typed text itself
        // (free-text model ids; also the whole flow for ollama's empty list).
        const picked = modelRows[idx]?.label ?? filter.trim();
        if (picked === "") return;
        setModel(picked);
        setFilter("");
        setStage(provider?.needs_key ? "key" : "confirm");
      } else if (stage === "key") {
        setStage("confirm");
      } else if (provider) {
        finish({ provider: provider.name, model, key });
      }
      return;
    }
    if (keyev.backspace || keyev.delete) {
      if (stage === "key") setKey((k) => k.slice(0, -1));
      else setFilter((f) => f.slice(0, -1));
      setIdx(0);
      return;
    }
    if (input && !keyev.ctrl && !keyev.meta) {
      if (stage === "key") setKey((k) => k + input);
      else if (stage !== "confirm") {
        setFilter((f) => f + input);
        setIdx(0);
      }
    }
  });

  return (
    <Box flexDirection="column" paddingX={1}>
      <PixelArt rows={KING_ART} />
      <Box borderStyle="round" borderColor={palette.teal} paddingX={1} alignSelf="flex-start">
        <Text bold>♚ Regent Setup</Text>
      </Box>
      {stage === "provider" && (
        <Step title="Provider" filter={filter} hint="↑↓ choose · type to filter · Enter select · Esc cancel">
          <SelectList rows={providerRows} selected={idx} />
        </Step>
      )}
      {stage === "model" && provider && (
        <Step
          title={`Model — ${provider.name}`}
          filter={filter}
          hint={
            modelRows.length > 0
              ? "↑↓ choose · type to filter (unmatched text = custom id) · Enter select · Esc back"
              : "type a model id (e.g. llama3.2) · Enter accept · Esc back"
          }
        >
          <SelectList rows={modelRows} selected={idx} />
        </Step>
      )}
      {stage === "key" && provider && (
        <Step title={`API key — ${provider.name}`} hint="Enter accepts · empty skips (set it in env later) · Esc back">
          <Text>
            {"  "}
            <Text color={palette.tealDim}>{provider.key_env}: </Text>
            {key === "" ? <Text color={palette.grey}>(empty — skip)</Text> : "•".repeat(key.length)}
          </Text>
        </Step>
      )}
      {stage === "confirm" && provider && (
        <Step title="Review" hint="Enter saves · Esc goes back">
          <Text>{`  provider  `}<Text color={palette.teal}>{provider.name}</Text></Text>
          <Text>{`  model     `}<Text color={palette.teal}>{model}</Text></Text>
          <Text>
            {`  api key   `}
            {key === "" ? <Text color={palette.grey}>not set — export {provider.key_env} later</Text> : "set"}
          </Text>
          <Text color={palette.grey}>{`  constitution: always on · view with \`regent persona\``}</Text>
        </Step>
      )}
    </Box>
  );
}

function Step(props: {
  readonly title: string;
  readonly hint: string;
  readonly filter?: string;
  readonly children: React.ReactNode;
}) {
  return (
    <Box flexDirection="column" marginTop={1}>
      <Text bold color={palette.teal}>
        {props.title}
        {props.filter ? <Text color={palette.grey}>  /{props.filter}</Text> : null}
      </Text>
      {props.children}
      <Text color={palette.grey}>{props.hint}</Text>
    </Box>
  );
}
