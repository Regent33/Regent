// Lazy singleton mermaid loader — mirrors highlighter.ts's pattern (load once,
// cache the promise) so a chat message with no diagrams never pulls the
// mermaid chunk. `securityLevel: 'strict'` sanitizes label HTML and drops
// click handlers, so the rendered SVG is safe to inject via
// dangerouslySetInnerHTML. Client-only: callers must be inside a 'use client'
// component and only call this after mount, or the dynamic import would
// otherwise be pulled into the server/static-export bundle.
type MermaidTheme = 'default' | 'dark';
let initPromise: Promise<typeof import('mermaid').default> | undefined;
let currentTheme: MermaidTheme = 'default';

function configure(mermaid: typeof import('mermaid').default, theme: MermaidTheme): void {
  mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme, fontFamily: 'inherit' });
  currentTheme = theme;
}

function getMermaid(theme: MermaidTheme): Promise<typeof import('mermaid').default> {
  initPromise ??= import('mermaid').then((mod) => {
    configure(mod.default, theme);
    return mod.default;
  });
  // Re-initialize only when the requested theme differs from the last one —
  // cheap, and it keeps the single cached instance (chat stays 'default';
  // Butler's full-screen diagram renders 'dark').
  return initPromise.then((mermaid) => {
    if (theme !== currentTheme) configure(mermaid, theme);
    return mermaid;
  });
}

let counter = 0;

/** Render mermaid `code` to an SVG string, or throw mermaid's own parse
 * error — the caller falls back to a raw code block with the error text. */
export async function renderMermaid(code: string, theme: MermaidTheme = 'default'): Promise<string> {
  const mermaid = await getMermaid(theme);
  counter += 1;
  const { svg } = await mermaid.render(`mermaid-${Date.now()}-${counter}`, code);
  return svg;
}
