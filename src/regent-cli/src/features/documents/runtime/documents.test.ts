import { describe, expect, test } from "bun:test";
import { discoverBrowser, renderPdf, screenshot } from "./browser.ts";
import { buildPptx } from "./presentation.ts";
import { dispatch } from "./renderCommand.ts";
import type { DeckSpec } from "./types.ts";

const THEME = {
  background: "FFFFFF",
  text: "171C2C",
  accent: "00A19B",
  muted: "667085",
  coverBackground: "171C2C",
  coverText: "FFFFFF",
  titleFont: "Arial",
  bodyFont: "Arial",
} as const;

function isZip(bytes: Uint8Array): boolean {
  return bytes[0] === 0x50 && bytes[1] === 0x4b; // "PK"
}

describe("presentation", () => {
  test("builds a valid multi-layout deck with chart, image and notes", async () => {
    const deck: DeckSpec = {
      theme: THEME,
      slides: [
        { layout: "cover", title: "Dynamic Deck", subtitle: "Not the same template" },
        {
          layout: "content",
          title: "Findings",
          bullets: ["One", "Two", "Three"],
          notes: "speak here",
        },
        {
          layout: "chart",
          title: "Growth",
          chart: {
            kind: "bar",
            series: [{ name: "Q", labels: ["A", "B", "C"], values: [3, 5, 2] }],
          },
        },
        {
          layout: "split",
          title: "Visual",
          bullets: ["evidence"],
          // 1x1 transparent PNG
          imageBase64:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
        },
        { layout: "section", title: "Next" },
      ],
    };
    const result = await buildPptx(deck);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(isZip(result.value)).toBe(true);
    expect(result.value.length).toBeGreaterThan(20_000);
    // The native chart part is named uncompressed in the zip directory.
    const asLatin1 = Buffer.from(result.value).toString("latin1");
    expect(asLatin1.includes("charts/chart")).toBe(true);
  });

  test("grid layout renders enumerated bullets as numbered cards", async () => {
    // Odd count exercises the full-width last-card path.
    const deck: DeckSpec = {
      theme: THEME,
      slides: [
        {
          layout: "grid",
          title: "Roadmap Overview",
          bullets: ["Foundations", "Core AI", "Advanced Electives", "Strategy", "Community"],
        },
      ],
    };
    const result = await buildPptx(deck);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(isZip(result.value)).toBe(true);
    expect(result.value.length).toBeGreaterThan(10_000);
  });
});

describe("dispatch", () => {
  test("pptx job returns base64 bytes", async () => {
    const result = await dispatch({
      kind: "pptx",
      deck: { theme: THEME, slides: [{ layout: "content", title: "Hi", bullets: ["x"] }] },
    });
    expect(result.ok).toBe(true);
    if (result.ok) expect(result.bytes.length).toBeGreaterThan(0);
  });

  test("empty deck is a typed bad-job error, not a crash", async () => {
    const result = await dispatch({ kind: "pptx", deck: { theme: THEME, slides: [] } });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("bad-job");
  });

  test("empty html is a typed bad-job error", async () => {
    const result = await dispatch({ kind: "pdf", html: "" });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("bad-job");
  });

  test("unknown kind is reported, not thrown", async () => {
    // deliberately malformed job
    const result = await dispatch({ kind: "gif" } as never);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.error.kind).toBe("unknown-kind");
  });
});

describe("browser", () => {
  test("discovery returns a well-formed result", () => {
    const found = discoverBrowser();
    if (found.ok) expect(found.value.length).toBeGreaterThan(0);
    else expect(found.error.kind).toBe("browser-missing");
  });

  // Only exercises a real render where a browser exists; skips cleanly otherwise
  // so CI without a browser stays green.
  test("renders HTML to a real PDF when a browser is installed", async () => {
    const found = discoverBrowser();
    if (!found.ok) return;
    const pdf = await renderPdf("<!doctype html><h1 style='color:#00A19B'>Hi</h1>");
    expect(pdf.ok).toBe(true);
    if (!pdf.ok) return;
    // "%PDF-" magic.
    expect(Array.from(pdf.value.slice(0, 5))).toEqual([0x25, 0x50, 0x44, 0x46, 0x2d]);
  }, 70_000);

  test("screenshots HTML to a PNG (headless) when a browser is installed", async () => {
    const found = discoverBrowser();
    if (!found.ok) return;
    const shot = await screenshot("<!doctype html><h1 style='color:#00A19B'>Preview</h1>");
    expect(shot.ok).toBe(true);
    if (!shot.ok) return;
    // PNG magic bytes.
    expect(Array.from(shot.value.slice(0, 4))).toEqual([0x89, 0x50, 0x4e, 0x47]);
  }, 70_000);
});
