'use client';
// THE markdown renderer for model output — react-markdown + GFM mapped onto
// the token layer (zero raw colors). Links open in the system browser via the
// opener plugin (never navigate the app window); plain window.open covers the
// non-shell dev case. Fenced code routes to CodeBlock (Shiki highlighting +
// copy + collapse, owns its own `<pre>`) except ```mermaid, which routes to
// MermaidDiagram instead; images route to ZoomableImage (click → lightbox);
// a recognized YouTube/OpenStreetMap link routes to the consent-gated
// EmbedCard instead of a plain anchor.
import { isValidElement, type ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openUrl } from '@tauri-apps/plugin-opener';
import { isTauri } from '@/shared/infrastructure/rpc/client';
import { CodeBlock } from '@/shared/ui/markdown/CodeBlock';
import { detectEmbed } from '@/shared/ui/markdown/embedDetect';
import { EmbedCard } from '@/shared/ui/markdown/EmbedCard';
import { MermaidDiagram } from '@/shared/ui/markdown/MermaidDiagram';
import { SpecDiagram, specFromCode } from '@/shared/ui/markdown/SpecDiagram';
import { ZoomableImage } from '@/shared/ui/markdown/ZoomableImage';

function openExternal(href: string | undefined) {
  if (href === undefined || !/^https?:\/\//.test(href)) return;
  if (isTauri()) void openUrl(href);
  else window.open(href, '_blank', 'noreferrer');
}

function textOf(children: unknown): string {
  if (typeof children === 'string') return children;
  if (Array.isArray(children)) return children.map(textOf).join('');
  return '';
}

/** Every fenced block reaches `pre` wrapping a single `code` child (mdast/hast
 * always nests it this way, language tag or not) — unlike `code`, which also
 * fires for inline spans, so extracting here is the reliable place to route
 * fenced blocks to CodeBlock and leave inline code alone. */
function PreBlock({ children }: { children?: ReactNode }) {
  const child = Array.isArray(children) ? children[0] : children;
  if (isValidElement<{ className?: string; children?: unknown }>(child)) {
    const match = /language-(\S+)/.exec(child.props.className ?? '');
    const lang = match?.[1] ?? '';
    const code = textOf(child.props.children);
    if (lang.toLowerCase() === 'mermaid') return <MermaidDiagram code={code} />;
    // A diagram spec (voice/butler emits ```json / ```present) draws as the
    // actual diagram here, not raw JSON; anything else stays a code block.
    const spec = specFromCode(lang, code);
    if (spec !== null) return <SpecDiagram spec={spec} />;
    return <CodeBlock language={lang} code={code} />;
  }
  return <pre className="overflow-x-auto">{children}</pre>;
}

export function Markdown({ text, muted = false }: { text: string; muted?: boolean }) {
  const tone = muted ? 'text-text-tertiary' : 'text-text-primary';
  return (
    <div className={`min-w-0 break-words text-sm leading-relaxed ${tone}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          p: (p) => <p className="my-1.5 whitespace-pre-wrap first:mt-0 last:mb-0" {...p} />,
          strong: (p) => <strong className="font-semibold" {...p} />,
          em: (p) => <em {...p} />,
          a: ({ href, children }) => {
            const descriptor = href !== undefined ? detectEmbed(href) : undefined;
            if (descriptor !== undefined) return <EmbedCard descriptor={descriptor} />;
            return (
              <a
                href={href}
                className="cursor-pointer text-accent underline underline-offset-2"
                onClick={(e) => {
                  e.preventDefault();
                  openExternal(href);
                }}
              >
                {children}
              </a>
            );
          },
          ul: (p) => <ul className="my-1.5 list-disc pl-5" {...p} />,
          ol: (p) => <ol className="my-1.5 list-decimal pl-5" {...p} />,
          li: (p) => <li className="my-0.5" {...p} />,
          h1: (p) => <h3 className="mb-1 mt-3 text-base font-semibold first:mt-0" {...p} />,
          h2: (p) => <h4 className="mb-1 mt-3 text-[15px] font-semibold first:mt-0" {...p} />,
          h3: (p) => <h5 className="mb-1 mt-2.5 text-sm font-semibold first:mt-0" {...p} />,
          h4: (p) => <h6 className="mb-1 mt-2 text-sm font-semibold first:mt-0" {...p} />,
          blockquote: (p) => (
            <blockquote className="my-1.5 border-l-2 border-stroke-primary pl-3 text-text-secondary" {...p} />
          ),
          hr: () => <hr className="my-3 border-stroke-tertiary" />,
          // Only inline spans reach `code` (fenced blocks are intercepted by
          // the `pre` override, which routes them to CodeBlock instead).
          code: ({ children }) => (
            <code className="rounded-[4px] bg-hover px-1 py-0.5 font-mono text-[0.85em]">{children}</code>
          ),
          pre: PreBlock,
          img: ({ src, alt }) => (typeof src === 'string' ? <ZoomableImage src={src} alt={alt} /> : null),
          table: (p) => (
            <div className="my-2 overflow-x-auto">
              <table className="w-full border-collapse text-xs" {...p} />
            </div>
          ),
          th: (p) => (
            <th className="border-b border-stroke-secondary px-2 py-1 text-left font-semibold" {...p} />
          ),
          td: (p) => <td className="border-b border-stroke-tertiary px-2 py-1 align-top" {...p} />,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
