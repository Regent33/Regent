// Strict wire protocol between the Rust `create_document` executor and this
// renderer sidecar. Exactly one JSON job arrives on stdin; exactly one JSON
// result leaves on stdout. Binary output (PDF/PPTX bytes) crosses the boundary
// base64-encoded — nothing else is ever written to stdout.

export type RenderJob = PdfJob | PptxJob | PreviewJob;

/** Screenshot HTML to a PNG for the vision feedback loop. Headless/background —
 * the report's own HTML, so the preview is faithful to the PDF. */
export interface PreviewJob {
  readonly kind: "preview";
  readonly html: string;
  readonly width?: number;
  readonly height?: number;
}

/** Render a complete, self-contained HTML document to a PDF via headless
 * Chromium. The HTML (Tera template + inlined CSS + inlined SVG charts) is built
 * on the Rust side; the browser is a pure HTML-to-PDF function here. */
export interface PdfJob {
  readonly kind: "pdf";
  readonly html: string;
  readonly page?: PageOptions;
}

export interface PageOptions {
  readonly landscape?: boolean;
}

/** Build an editable PowerPoint from a themed, multi-layout deck spec via
 * PptxGenJS. This is where deck-to-deck visual variety lives. */
export interface PptxJob {
  readonly kind: "pptx";
  readonly deck: DeckSpec;
}

export interface DeckSpec {
  readonly theme: DeckTheme;
  readonly slides: readonly DeckSlide[];
}

/** A palette + font pairing. Hex colors carry no leading '#'. Different themes
 * per deck are the whole point — they end "every deck looks identical". */
export interface DeckTheme {
  readonly background: string;
  readonly text: string;
  readonly accent: string;
  readonly muted: string;
  readonly coverBackground: string;
  readonly coverText: string;
  readonly titleFont: string;
  readonly bodyFont: string;
}

export type SlideLayout = "cover" | "content" | "section" | "split" | "chart" | "grid" | "blank";

export interface DeckSlide {
  readonly layout: SlideLayout;
  readonly title?: string;
  readonly subtitle?: string;
  readonly bullets?: readonly string[];
  readonly notes?: string;
  /** PNG bytes, base64 (no data: prefix). The Rust hydrator normalizes to PNG. */
  readonly imageBase64?: string;
  readonly chart?: ChartSpec;
}

export type ChartKind = "bar" | "line" | "pie";

export interface ChartSpec {
  readonly kind: ChartKind;
  readonly series: readonly ChartSeries[];
}

export interface ChartSeries {
  readonly name: string;
  readonly labels: readonly string[];
  readonly values: readonly number[];
}

export type RenderResult =
  | { readonly ok: true; readonly bytes: string }
  | { readonly ok: false; readonly error: RenderError };

export interface RenderError {
  readonly kind: string;
  readonly message: string;
}
