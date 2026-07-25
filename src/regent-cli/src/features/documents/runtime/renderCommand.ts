// The hidden `regent __render` subcommand: the Rust create_document executor
// spawns it, pipes one JSON render job to stdin, and reads one JSON result from
// stdout. Bytes come back base64. This process writes NOTHING else to stdout —
// diagnostics go to stderr — so the Rust side can parse stdout verbatim.

import { err, failure, ok, type Result } from "@shared/kernel/result.ts";
import { renderPdf, screenshot } from "./browser.ts";
import { buildPptx } from "./presentation.ts";
import type { RenderJob, RenderResult } from "./types.ts";

const MAX_INPUT_BYTES = 96 * 1024 * 1024;

export async function renderCommand(): Promise<number> {
  const input = await readStdin();
  if (!input.ok) {
    emit(errorResult(input.error.kind, input.error.message));
    return 1;
  }
  let job: RenderJob;
  try {
    job = JSON.parse(input.value) as RenderJob;
  } catch (cause) {
    emit(errorResult("bad-json", `render job is not valid JSON: ${String(cause)}`));
    return 1;
  }
  const result = await dispatch(job);
  emit(result);
  return result.ok ? 0 : 1;
}

/** Route a job to its renderer and encode the bytes. Exported for tests. */
export async function dispatch(job: RenderJob): Promise<RenderResult> {
  switch (job?.kind) {
    case "pdf": {
      if (typeof job.html !== "string" || job.html.length === 0) {
        return errorResult("bad-job", "pdf job needs a non-empty `html` string");
      }
      return encode(await renderPdf(job.html, job.page));
    }
    case "pptx": {
      const slides = job.deck?.slides;
      if (!Array.isArray(slides) || slides.length === 0) {
        return errorResult("bad-job", "pptx job needs a non-empty `deck.slides` array");
      }
      return encode(await buildPptx(job.deck));
    }
    case "preview": {
      if (typeof job.html !== "string" || job.html.length === 0) {
        return errorResult("bad-job", "preview job needs a non-empty `html` string");
      }
      return encode(await screenshot(job.html, job));
    }
    default:
      return errorResult("unknown-kind", `unknown render kind: ${describeKind(job)}`);
  }
}

function encode(bytes: Result<Uint8Array>): RenderResult {
  if (!bytes.ok) return errorResult(bytes.error.kind, bytes.error.message);
  return { ok: true, bytes: Buffer.from(bytes.value).toString("base64") };
}

function errorResult(kind: string, message: string): RenderResult {
  return { ok: false, error: { kind, message } };
}

function describeKind(job: unknown): string {
  const kind = (job as { kind?: unknown })?.kind;
  return typeof kind === "string" ? kind : String(kind);
}

function emit(result: RenderResult): void {
  process.stdout.write(JSON.stringify(result));
}

async function readStdin(): Promise<Result<string>> {
  const chunks: Buffer[] = [];
  let total = 0;
  try {
    for await (const chunk of process.stdin) {
      const buf = chunk as Buffer;
      total += buf.length;
      if (total > MAX_INPUT_BYTES) {
        return err(failure("input-too-large", "render job exceeds the 96MB stdin cap"));
      }
      chunks.push(buf);
    }
  } catch (cause) {
    return err(failure("stdin-read-failed", "could not read the render job from stdin", cause));
  }
  return ok(Buffer.concat(chunks).toString("utf8"));
}
