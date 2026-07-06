'use client';
// THE markdown renderer for model output — react-markdown + GFM mapped onto
// the token layer (zero raw colors). Links open in the system browser via the
// opener plugin (never navigate the app window); plain window.open covers the
// non-shell dev case.
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openUrl } from '@tauri-apps/plugin-opener';
import { isTauri } from '@/shared/infrastructure/rpc/client';

function openExternal(href: string | undefined) {
  if (href === undefined || !/^https?:\/\//.test(href)) return;
  if (isTauri()) void openUrl(href);
  else window.open(href, '_blank', 'noreferrer');
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
          a: ({ href, children }) => (
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
          ),
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
          code: ({ className, children }) => {
            const block = typeof className === 'string' && className.includes('language-');
            if (block) return <code className={className}>{children}</code>;
            return (
              <code className="rounded-[4px] bg-hover px-1 py-0.5 font-mono text-[0.85em]">
                {children}
              </code>
            );
          },
          pre: (p) => (
            <pre
              className="my-2 overflow-x-auto rounded-md bg-hover p-3 font-mono text-xs leading-relaxed"
              {...p}
            />
          ),
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
