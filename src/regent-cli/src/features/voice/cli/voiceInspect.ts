// Read-only `regent voice` subcommands: test / status / models. Each is a thin
// deacon RPC call + a formatted print.
import { out, printError } from "@app/cli/runtime.ts";
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { style } from "@shared/ui/style.ts";

export async function voiceTest(client: IRpcClient): Promise<number> {
  out(style.grey("synthesizing a test phrase…"));
  const res = await client.call<{ provider: string; bytes: number; format: string }>(
    "voice.test",
    {},
    60_000,
  );
  if (!res.ok) {
    printError(res.error.message);
    out(style.grey("  check `regent voice status` — provider/key/server reachable?"));
    return 1;
  }
  const v = res.value;
  out(style.pass(`✓ TTS works — ${v.provider} produced ${v.bytes} bytes of ${v.format} audio`));
  return 0;
}

interface VoiceStatus {
  enabled: boolean;
  asr: { provider: string; model: string; available: boolean };
  tts: { provider: string; model: string; available: boolean };
  vision: { input_mode: string };
  call: { fast_model: string };
}

export async function voiceStatus(client: IRpcClient): Promise<number> {
  const res = await client.call<VoiceStatus>("voice.status", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const s = res.value;
  const dot = (ok: boolean): string => (ok ? style.teal("●") : style.grey("○"));
  out(style.heading("Voice"));
  out(`  ${"enabled".padEnd(8)} ${s.enabled ? style.teal("yes") : style.grey("no")}`);
  out(`  ${"asr".padEnd(8)} ${dot(s.asr.available)} ${s.asr.provider}/${s.asr.model}`);
  out(`  ${"tts".padEnd(8)} ${dot(s.tts.available)} ${s.tts.provider}/${s.tts.model}`);
  out(`  ${"vision".padEnd(8)} ${s.vision.input_mode}`);
  if (s.call.fast_model) out(`  ${"fast".padEnd(8)} ${s.call.fast_model}`);
  if (!s.enabled) out(style.grey("\n  enable with: regent voice setup"));
  return 0;
}

interface VoiceModels {
  asr: { configured: { provider: string; model: string }; builtins: string[] };
  tts: { configured: { provider: string; model: string }; builtins: string[] };
}

export async function voiceModels(client: IRpcClient): Promise<number> {
  const res = await client.call<VoiceModels>("voice.models", {}, 15_000);
  if (!res.ok) {
    printError(res.error.message);
    return 1;
  }
  const v = res.value;
  out(style.heading("Voice providers"));
  out(
    `  ${"asr".padEnd(4)} ${style.value(`${v.asr.configured.provider}/${v.asr.configured.model}`)}`,
  );
  out(`       ${style.grey(`built-in: ${v.asr.builtins.join(", ")}`)}`);
  out(
    `  ${"tts".padEnd(4)} ${style.value(`${v.tts.configured.provider}/${v.tts.configured.model}`)}`,
  );
  out(`       ${style.grey(`built-in: ${v.tts.builtins.join(", ")}`)}`);
  return 0;
}
