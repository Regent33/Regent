// Lazy singleton mermaid loader — mirrors highlighter.ts's pattern (load once,
// cache the promise) so a chat message with no diagrams never pulls the
// mermaid chunk. `securityLevel: 'strict'` sanitizes label HTML and drops
// click handlers, so the rendered SVG is safe to inject via
// dangerouslySetInnerHTML. Client-only: callers must be inside a 'use client'
// component and only call this after mount, or the dynamic import would
// otherwise be pulled into the server/static-export bundle.
export type MermaidTheme = 'default' | 'dark';
let initPromise: Promise<typeof import('mermaid').default> | undefined;
let currentTheme: MermaidTheme = 'default';

function configure(mermaid: typeof import('mermaid').default, theme: MermaidTheme): void {
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'strict',
    theme,
    fontFamily: 'inherit',
    // Dagre lays each edge label out as a dummy node on a rank of its own, so
    // labels on edges spanning the SAME two ranks sit side by side and are
    // separated by nodeSpacing. At the stock 50 a hub node (one nameserver
    // with "ask authoritative NS" / "returns IP A record" / "holds DNS
    // records" all meeting at it) crushed them into one band where they
    // overlapped and read as clipped text — so nodeSpacing is the knob that
    // matters here. rankSpacing rises only modestly: the stage is
    // height-constrained (max-h on the svg), so extra vertical growth is paid
    // for by scaling the whole diagram — and its text — down.
    flowchart: { nodeSpacing: 100, rankSpacing: 62, padding: 16 },
    themeVariables: {
      // Label chips matched to the app's own warm neutrals (--surface, see
      // globals.css). Mermaid's dark theme paints them #e8e8e8 — a bright,
      // cool grey that reads as a stack of sticky notes over a charcoal
      // stage and makes every collision maximally loud. Keeping them near
      // the stage colour lets the label text carry the meaning.
      edgeLabelBackground: theme === 'dark' ? 'hsl(35deg 8% 15%)' : 'hsl(36deg 32% 93%)',
    },
  });
  currentTheme = theme;
}

function getMermaid(theme: MermaidTheme): Promise<typeof import('mermaid').default> {
  initPromise ??= import('mermaid').then((mod) => {
    configure(mod.default, theme);
    return mod.default;
  });
  // Re-initialize only when the requested theme differs from the last one —
  // cheap, and it keeps the single cached instance (chat stays 'default';
  // Butler follows the effective app theme).
  return initPromise.then((mermaid) => {
    if (theme !== currentTheme) configure(mermaid, theme);
    return mermaid;
  });
}

let counter = 0;

/** Resolve the effective app theme when a caller does not provide one. The
 * root attribute wins; its absence means system mode. Kept pure at the seam so
 * artifact preview/lightbox regressions are testable without rendering React. */
export function resolveMermaidTheme(
  explicit?: MermaidTheme,
  rootTheme: string | null = typeof document === 'undefined'
    ? null
    : document.documentElement.getAttribute('data-theme'),
  systemDark = typeof matchMedia === 'function' && matchMedia('(prefers-color-scheme: dark)').matches,
): MermaidTheme {
  if (explicit) return explicit;
  if (rootTheme === 'dark') return 'dark';
  if (rootTheme === 'light') return 'default';
  return systemDark ? 'dark' : 'default';
}

/** Render mermaid `code` to an SVG string, or throw mermaid's own parse
 * error — the caller falls back to a raw code block with the error text. */
export async function renderMermaid(code: string, theme?: MermaidTheme): Promise<string> {
  const effectiveTheme = resolveMermaidTheme(theme);
  const mermaid = await getMermaid(effectiveTheme);
  counter += 1;
  const { svg } = await mermaid.render(`mermaid-${Date.now()}-${counter}`, code);
  return svg;
}
