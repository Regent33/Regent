// Headless-Chromium HTML-to-PDF. We drive whatever browser is already installed
// (Edge / Chrome / Chromium) with `--headless=new --print-to-pdf` — the same
// approach the documents skill already documents — so nothing bundles a ~200MB
// browser and there is no Playwright driver to package into the bun binary.
// `REGENT_CHROMIUM_PATH` overrides discovery.

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { type Result, err, failure, ok } from "@shared/kernel/result.ts";
import type { PageOptions } from "./types.ts";

const BASE_FLAGS = ["--headless=new", "--disable-gpu", "--no-pdf-header-footer"];
const RENDER_TIMEOUT_MS = 60_000;

/** Locate an installed Chromium-family browser, or a clear typed error. */
export function discoverBrowser(): Result<string> {
  const override = process.env.REGENT_CHROMIUM_PATH;
  if (override) {
    return existsSync(override)
      ? ok(override)
      : err(failure("browser-missing", `REGENT_CHROMIUM_PATH does not exist: ${override}`));
  }
  for (const candidate of browserCandidates()) {
    if (existsSync(candidate)) return ok(candidate);
  }
  return err(
    failure(
      "browser-missing",
      "no Chrome/Edge/Chromium found — install one or set REGENT_CHROMIUM_PATH to a browser executable",
    ),
  );
}

function browserCandidates(): readonly string[] {
  if (process.platform === "win32") {
    const pf = process.env.PROGRAMFILES ?? "C:\\Program Files";
    const pfx86 = process.env["PROGRAMFILES(X86)"] ?? "C:\\Program Files (x86)";
    const local = process.env.LOCALAPPDATA;
    return [
      `${pfx86}\\Microsoft\\Edge\\Application\\msedge.exe`,
      `${pf}\\Microsoft\\Edge\\Application\\msedge.exe`,
      `${pf}\\Google\\Chrome\\Application\\chrome.exe`,
      `${pfx86}\\Google\\Chrome\\Application\\chrome.exe`,
      ...(local ? [`${local}\\Google\\Chrome\\Application\\chrome.exe`] : []),
    ];
  }
  if (process.platform === "darwin") {
    return [
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];
  }
  return [
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/snap/bin/chromium",
  ];
}

/** Render a complete HTML document to PDF bytes. */
export async function renderPdf(html: string, page?: PageOptions): Promise<Result<Uint8Array>> {
  const browser = discoverBrowser();
  if (!browser.ok) return browser;

  let dir: string | undefined;
  try {
    dir = await mkdtemp(join(tmpdir(), "regent-render-"));
    const htmlPath = join(dir, "page.html");
    const pdfPath = join(dir, "out.pdf");
    // A throwaway profile dir keeps this render from failing when the user
    // already has the browser open (the default profile is locked).
    const userDataDir = join(dir, "profile");
    await writeFile(htmlPath, html, "utf8");

    const flags = [...BASE_FLAGS];
    if (page?.landscape) flags.push("--landscape");
    const args = [
      ...flags,
      `--user-data-dir=${userDataDir}`,
      `--print-to-pdf=${pdfPath}`,
      pathToFileURL(htmlPath).href,
    ];

    const launched = await runBrowser(browser.value, args);
    if (!launched.ok) return launched;
    if (!existsSync(pdfPath)) {
      return err(failure("pdf-empty", "browser exited cleanly but wrote no PDF"));
    }
    const bytes = await readFile(pdfPath);
    return ok(new Uint8Array(bytes));
  } catch (cause) {
    return err(failure("pdf-render-failed", "HTML-to-PDF rendering failed", cause));
  } finally {
    if (dir) await rm(dir, { recursive: true, force: true }).catch(() => undefined);
  }
}

export interface ShotOptions {
  readonly width?: number;
  readonly height?: number;
}

/** Screenshot a complete HTML document to PNG bytes — fully headless (no window,
 * no focus steal), so it is safe to run while the user is using the machine. Used
 * for the vision feedback loop's report preview. */
export async function screenshot(html: string, opts?: ShotOptions): Promise<Result<Uint8Array>> {
  const browser = discoverBrowser();
  if (!browser.ok) return browser;

  let dir: string | undefined;
  try {
    dir = await mkdtemp(join(tmpdir(), "regent-shot-"));
    const htmlPath = join(dir, "page.html");
    const pngPath = join(dir, "shot.png");
    const userDataDir = join(dir, "profile");
    await writeFile(htmlPath, html, "utf8");

    // A4 at ~150dpi captures the cover + first content band for a QA glance.
    const width = opts?.width ?? 1240;
    const height = opts?.height ?? 1754;
    const args = [
      ...BASE_FLAGS,
      "--hide-scrollbars",
      `--window-size=${width},${height}`,
      `--user-data-dir=${userDataDir}`,
      `--screenshot=${pngPath}`,
      pathToFileURL(htmlPath).href,
    ];

    const launched = await runBrowser(browser.value, args);
    if (!launched.ok) return launched;
    if (!existsSync(pngPath)) {
      return err(failure("shot-empty", "browser exited cleanly but wrote no screenshot"));
    }
    const bytes = await readFile(pngPath);
    return ok(new Uint8Array(bytes));
  } catch (cause) {
    return err(failure("screenshot-failed", "HTML screenshot failed", cause));
  } finally {
    if (dir) await rm(dir, { recursive: true, force: true }).catch(() => undefined);
  }
}

function runBrowser(exe: string, args: readonly string[]): Promise<Result<void>> {
  return new Promise((resolve) => {
    const child = spawn(exe, [...args], { stdio: ["ignore", "ignore", "pipe"] });
    let stderr = "";
    child.stderr?.on("data", (chunk) => {
      stderr += String(chunk);
    });
    const timer = setTimeout(() => {
      child.kill();
      resolve(err(failure("browser-timeout", `browser render exceeded ${RENDER_TIMEOUT_MS}ms`)));
    }, RENDER_TIMEOUT_MS);
    child.on("error", (cause) => {
      clearTimeout(timer);
      resolve(err(failure("browser-spawn-failed", `cannot launch ${exe}`, cause)));
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      if (code === 0) return resolve(ok(undefined));
      resolve(err(failure("browser-failed", `browser exited ${code}: ${stderr.slice(0, 500)}`)));
    });
  });
}
