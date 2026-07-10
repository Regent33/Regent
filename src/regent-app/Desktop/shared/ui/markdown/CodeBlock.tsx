'use client';
// A fenced code block: language label + copy button header, Shiki-highlighted
// body (falls back to a plain <pre> while the highlighter loads or for an
// unsupported language), collapsing behind ExpandableBlock past ~400px.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { copyText } from '@/shared/infrastructure/clipboard';
import { CheckIcon, CopyIcon } from '@/shared/ui/icons';
import { ExpandableBlock } from '@/shared/ui/markdown/ExpandableBlock';
import { highlightCode } from '@/shared/ui/markdown/highlighter';

function CopyButton({ text }: { text: string }) {
  const s = t().chat.markdown;
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(id);
  }, [copied]);

  return (
    <button
      type="button"
      aria-label={copied ? s.copied : s.copyCode}
      onClick={() => {
        void copyText(text).then((ok) => setCopied(ok));
      }}
      className="shrink-0 rounded-[4px] p-1 text-text-tertiary transition-colors hover:bg-stroke-secondary hover:text-text-primary"
    >
      {copied ? <CheckIcon className="size-3.5" /> : <CopyIcon className="size-3.5" />}
    </button>
  );
}

export function CodeBlock({ language, code }: { language: string; code: string }) {
  const trimmed = code.replace(/\n+$/, '');
  const label = language.trim().toLowerCase();
  const [html, setHtml] = useState<string | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    setHtml(undefined);
    void highlightCode(trimmed, label).then((result) => {
      if (alive) setHtml(result);
    });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trimmed, label]);

  return (
    <div className="my-2 overflow-hidden rounded-md bg-hover">
      <div className="flex items-center justify-between gap-2 px-3 py-1.5">
        <span className="font-mono text-[11px] uppercase tracking-[0.04em] text-text-tertiary">
          {label !== '' ? label : 'text'}
        </span>
        <CopyButton text={trimmed} />
      </div>
      <ExpandableBlock>
        {html !== undefined ? (
          <div
            className="[&>pre]:!m-0 [&>pre]:!bg-transparent [&>pre]:overflow-x-auto [&>pre]:px-3 [&>pre]:pb-3 [&>pre]:font-mono [&>pre]:text-xs [&>pre]:leading-relaxed"
            dangerouslySetInnerHTML={{ __html: html }}
          />
        ) : (
          <pre className="m-0 overflow-x-auto px-3 pb-3 font-mono text-xs leading-relaxed text-text-primary">
            {trimmed}
          </pre>
        )}
      </ExpandableBlock>
    </div>
  );
}
