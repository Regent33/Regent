// `regent doctor` — verifies the installation end to end: deacon binary,
// REGENT_HOME, the EFFECTIVE provider/model/endpoint + active API key (the #1
// cause of HTTP 401), spawn → health → config.get.
//
// Three severities, not two. A posture warning ("your shell env is shadowing
// the .env key") is not a broken install, and failing the health command on one
// teaches people to ignore the health command. `--strict` is for the CI user
// who does want warnings to fail; the default exit code is unchanged.
import { mkdirSync } from "node:fs";
import { parseFlags, unknownFlags } from "@app/cli/args.ts";
import { EXIT } from "@app/cli/exit.ts";
import { CLI_VERSION } from "@app/cli/help.ts";
import { out, printError } from "@app/cli/runtime.ts";
import {
  activeProviderKey,
  maskKey,
  probeLocalProviderEndpoint,
  readProviderInfo,
} from "@features/doctor/diagnostics.ts";
import { checkForUpdateNotice } from "@features/update/data/checkForUpdate.ts";
import { connectHealthyDeacon } from "@shared/infrastructure/deacon/connect.ts";
import { deaconCandidates, regentHome } from "@shared/infrastructure/deacon/locate.ts";
import { style } from "@shared/ui/style.ts";

export type Severity = "ok" | "warn" | "fail";
export interface Check {
  readonly check: string;
  readonly severity: Severity;
  readonly detail: string;
}

/** The verdict rule, kept pure so the exit-code contract is testable. */
export function doctorVerdict(
  checks: readonly Check[],
  strict: boolean,
): { status: Severity; code: number } {
  const status: Severity = checks.some((c) => c.severity === "fail")
    ? "fail"
    : checks.some((c) => c.severity === "warn")
      ? "warn"
      : "ok";
  const failing = status === "fail" || (status === "warn" && strict);
  return { status, code: failing ? EXIT.failure : EXIT.ok };
}

const MARK: Record<Severity, (s: string) => string> = {
  ok: style.pass,
  warn: style.warn,
  fail: style.fail,
};
const GLYPH: Record<Severity, string> = { ok: "✓", warn: "!", fail: "✗" };

/** Collect first, render once: the same run has to serve text and JSON. */
class Report {
  readonly checks: Check[] = [];
  add(severity: Severity, check: string, detail: string): void {
    this.checks.push({ check, severity, detail });
  }
}

function render(report: Report, json: boolean, strict: boolean): number {
  const { status, code } = doctorVerdict(report.checks, strict);
  if (json) {
    out(
      JSON.stringify({ cli_version: CLI_VERSION, status, strict, checks: report.checks }, null, 2),
    );
    return code;
  }
  out(`regent doctor (cli ${CLI_VERSION})\n`);
  for (const c of report.checks) {
    out(`  ${MARK[c.severity](GLYPH[c.severity])} ${c.check.padEnd(18)} ${c.detail}`);
  }
  if (status === "fail") printError("doctor found problems");
  else if (code !== EXIT.ok) printError("doctor found warnings (--strict)");
  else out(`\n${style.pass(status === "warn" ? "no failures" : "all checks passed")}`);
  return code;
}

export async function doctorCommand(profile: string, args: string[] = []): Promise<number> {
  const spec = { strict: { type: "boolean" }, json: { type: "boolean" } } as const;
  const bad = unknownFlags(args, spec);
  if (bad.length > 0) {
    // Silently ignoring `--strcit` means reporting a clean bill of health from
    // a check the caller believed was strict.
    printError(`unknown option: ${bad.join(" ")}   (regent doctor [--strict] [--json])`);
    return EXIT.usage;
  }
  const { values } = parseFlags(args, spec);
  const strict = values.strict === true;
  const json = values.json === true;
  const r = new Report();

  const candidates = deaconCandidates();
  if (candidates.length === 0) {
    r.add("fail", "deacon binary", "not found (set REGENT_DEACON_PATH or build regent-deacon)");
    return render(r, json, strict);
  }
  const [firstCandidate] = candidates;
  r.add("ok", "deacon binary", `${candidates.length} candidate(s); first: ${firstCandidate}`);

  const home = regentHome(profile);
  try {
    mkdirSync(home, { recursive: true });
    r.add("ok", "REGENT_HOME", home);
  } catch (e) {
    r.add("fail", "REGENT_HOME", `${home}: ${e instanceof Error ? e.message : String(e)}`);
  }

  // Effective provider/model/endpoint (read straight from config.yaml).
  const providerInfo = readProviderInfo(home);
  const { provider, model, endpoint } = providerInfo;
  r.add("ok", "provider", `${provider} · ${model} · ${endpoint}`);

  // Resolve the exact key variable selected by config.providers. A generic
  // REGENT_API_KEY must not make doctor green when the registry requires (for
  // example) NVIDIA_API_KEY, and an explicitly keyless local server is valid.
  const activeKey = activeProviderKey(home, providerInfo);
  if (!activeKey.value && providerInfo.needsKey) {
    r.add(
      "fail",
      "API key",
      `no ${providerInfo.keyCandidates[0] ?? "provider key"} in shell env or .env — run \`regent setup\``,
    );
  } else if (!activeKey.value) {
    r.add("ok", "API key", "not required for this local provider");
  } else {
    r.add("ok", "API key", `${maskKey(activeKey.value)} (from ${activeKey.source})`);
    if (activeKey.shadowed) {
      r.add(
        "warn",
        "API key",
        `shell ${activeKey.shadowed} overrides a different .env value — unset it (PowerShell: \`Remove-Item Env:${activeKey.shadowed}\`) to use the saved key`,
      );
    }
  }

  // A healthy deacon only proves Regent's control plane is running. For a
  // self-hosted model, separately prove the configured inference endpoint can
  // answer model discovery. This is keyless and does not spend completion
  // tokens; a dead LM Studio/Ollama/etc. must make Doctor fail, including JSON.
  if (providerInfo.isLocal) {
    const endpointProbe = await probeLocalProviderEndpoint(providerInfo, {
      ...(activeKey.value ? { apiKey: activeKey.value } : {}),
    });
    r.add(endpointProbe.ok ? "ok" : "fail", "provider endpoint", endpointProbe.detail);
  }

  // Spawn + health-probe every candidate; report the one that actually answered
  // (which may not be the first if a stale pinned binary boots then dies).
  const connected = await connectHealthyDeacon(home);
  if (!connected.ok) {
    r.add("fail", "health round-trip", connected.error.message);
    return render(r, json, strict);
  }
  const client = connected.value.client;
  r.add("ok", "health round-trip", `ok via ${connected.value.path}`);

  const cfg = await client.call("config.get", {}, 15_000);
  if (cfg.ok) r.add("ok", "config.yaml", "loads and validates");
  else r.add("fail", "config.yaml", cfg.error.message);

  // Notify-only update check (ADR-041, Phase 0): informational, never a failure.
  // A missing method (old deacon) or any error yields no line — doctor's verdict
  // is unaffected.
  const updateNotice = await checkForUpdateNotice(client, CLI_VERSION);
  await client.close();
  if (updateNotice) r.add("warn", "update", updateNotice);

  return render(r, json, strict);
}
