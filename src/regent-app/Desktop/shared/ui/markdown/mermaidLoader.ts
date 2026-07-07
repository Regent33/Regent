// Lazy singleton mermaid loader — mirrors highlighter.ts's pattern (load once,
// cache the promise) so a chat message with no diagrams never pulls the
// mermaid chunk. `securityLevel: 'strict'` sanitizes label HTML and drops
// click handlers, so the rendered SVG is safe to inject via
// dangerouslySetInnerHTML. Client-only: callers must be inside a 'use client'
// component and only call this after mount, or the dynamic import would
// otherwise be pulled into the server/static-export bundle.
let initPromise: Promise<typeof import('mermaid').default> | undefined;

function getMermaid(): Promise<typeof import('mermaid').default> {
  initPromise ??= import('mermaid').then((mod) => {
    const mermaid = mod.default;
    mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', theme: 'default', fontFamily: 'inherit' });
    return mermaid;
  });
  return initPromise;
}

let counter = 0;

/** Render mermaid `code` to an SVG string, or throw mermaid's own parse
 * error — the caller falls back to a raw code block with the error text. */
export async function renderMermaid(code: string): Promise<string> {
  const mermaid = await getMermaid();
  counter += 1;
  const { svg } = await mermaid.render(`mermaid-${Date.now()}-${counter}`, code);
  return svg;
}
