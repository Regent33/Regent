// One transcript row — user bubbles are a quiet right-aligned block,
// assistant replies flat (flat-not-boxed), thinking/tool rows quiet activity
// lines (Hermes-style per-turn structure), approvals an actionable card,
// errors verbatim.
import { useMemo } from 'react';
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { Markdown } from '@/shared/ui/Markdown';
import { SpecDiagram } from '@/shared/ui/markdown/SpecDiagram';
import {
  ChevronDownIcon,
  ErrorIcon,
  PaperclipIcon,
  WorktreeIcon,
  WrenchIcon,
} from '@/shared/ui/icons';
import { t } from '@/shared/i18n/t';
import type { TranscriptItem } from '@/shared/kernel/transcript';
import { extractPresentSpec } from '@/shared/diagram/presentSpec';
import { diffLines } from '@/shared/ui/diffLines';

export interface MessageRowProps {
  item: TranscriptItem;
  onApproval?: (approved: boolean) => void;
}

export function MessageRow({ item, onApproval }: MessageRowProps) {
  const s = t().chat.transcript;

  if (item.kind === 'user') {
    const attachments = item.attachments ?? [];
    const chips = [
      ...attachments.map((name) => ({ name, editor: false })),
      ...(item.context === undefined ? [] : [{ name: item.context, editor: true }]),
    ];
    return (
      <div className="flex flex-col items-end gap-1">
        {chips.length > 0 && (
          <ul className="flex max-w-[70%] flex-wrap justify-end gap-1">
            {chips.map((chip, index) => (
              <li
                key={`${chip.name}-${index}`}
                title={chip.name}
                // Capped, not full-bleed: a long PDF name stretched the chip
                // across the whole column and read as a second message.
                className="flex max-w-[16rem] items-center gap-1 rounded-md border border-border-subtle bg-surface-raised px-2 py-1 text-[11px] text-text-secondary"
              >
                {chip.editor ? (
                  <WorktreeIcon className="size-3 shrink-0 text-text-tertiary" />
                ) : (
                  <PaperclipIcon className="size-3 shrink-0 text-text-tertiary" />
                )}
                <span className="truncate">{chip.name}</span>
              </li>
            ))}
          </ul>
        )}
        {/* An attachment-only message has no words — don't paint an empty bubble. */}
        {item.text !== '' && (
          <p className="max-w-[70%] whitespace-pre-wrap break-words rounded-md bg-hover px-3 py-2 text-sm text-text-primary">
            {item.text}
          </p>
        )}
      </div>
    );
  }

  if (item.kind === 'assistant') {
    // Butler persists its full reply (including the leading diagram JSON) in
    // the shared session. Extract it explicitly on replay so reopening that
    // session in main chat reconstructs the same visual. This does not depend
    // on react-markdown preserving an exact code-fence shape; ordinary JSON
    // and Mermaid blocks still flow through Markdown unchanged.
    const { spec, text } = extractPresentSpec(item.text);
    return (
      <div>
        {spec !== null && <SpecDiagram spec={spec} />}
        {text !== '' && <Markdown text={text} />}
        {item.streaming && <Loader className="mt-1" />}
      </div>
    );
  }

  if (item.kind === 'thinking') {
    // Native <details>, collapsed by default — thinking is skimmable noise
    // until the user asks for it.
    return (
      <details className="group">
        <summary className="flex cursor-pointer list-none items-center gap-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-text-tertiary hover:text-text-secondary [&::-webkit-details-marker]:hidden">
          <ChevronDownIcon className="size-3 -rotate-90 transition-transform group-open:rotate-0" />
          {s.thinking}
        </summary>
        <div className="mt-0.5">
          <Markdown muted text={item.text} />
        </div>
      </details>
    );
  }

  if (item.kind === 'tool') {
    if (item.code !== undefined) {
      return (
        <details className="group">
          <summary className="flex cursor-pointer list-none items-center gap-1.5 text-xs text-text-tertiary hover:text-text-secondary [&::-webkit-details-marker]:hidden">
            <ChevronDownIcon className="size-3 -rotate-90 transition-transform group-open:rotate-0" />
            <WrenchIcon className="size-3.5 shrink-0" />
            <span className="shrink-0">{item.name}</span>
            {item.detail !== undefined && (
              <span className="truncate font-mono text-[11px] text-text-tertiary/80">{item.detail}</span>
            )}
            {item.adds !== undefined && (
              <span className="shrink-0 font-mono text-[11px] text-accent">+{item.adds}</span>
            )}
            {!item.done && <Loader />}
            {item.done && item.isError === true && <ErrorIcon className="size-3.5 shrink-0 text-danger" />}
          </summary>
          <div className="mt-1.5 overflow-hidden rounded-md border border-border-subtle bg-surface-raised">
            {item.code.kind === 'replace' && item.code.before !== undefined && item.code.after !== undefined ? (
              <DiffView before={item.code.before} after={item.code.after} />
            ) : (
              <>
                {item.code.before !== undefined && <CodeDisclosure label="Before" text={item.code.before} />}
                {item.code.after !== undefined && <CodeDisclosure label="Code" text={item.code.after} />}
              </>
            )}
            {item.code.patch !== undefined && <CodeDisclosure label="Patch" text={item.code.patch} />}
          </div>
        </details>
      );
    }
    return (
      <p className="flex items-center gap-1.5 text-xs text-text-tertiary">
        <WrenchIcon className="size-3.5 shrink-0" />
        <span className="shrink-0">{item.name}</span>
        {item.detail !== undefined && (
          <span className="truncate font-mono text-[11px] text-text-tertiary/80">{item.detail}</span>
        )}
        {item.adds !== undefined && <span className="shrink-0 font-mono text-[11px] text-accent">+{item.adds}</span>}
        {item.dels !== undefined && item.dels > 0 && (
          <span className="shrink-0 font-mono text-[11px] text-danger">−{item.dels}</span>
        )}
        {!item.done && <Loader />}
        {item.done && item.isError === true && <ErrorIcon className="size-3.5 shrink-0 text-danger" />}
      </p>
    );
  }

  if (item.kind === 'notice') {
    return (
      <p
        className={`text-xs ${item.tone === 'warn' ? 'text-danger' : 'text-text-tertiary'}`}
        role="status"
      >
        {item.text}
      </p>
    );
  }

  if (item.kind === 'approval') {
    return (
      <div className="rounded-md bg-hover px-3 py-2.5">
        <p className="text-xs font-semibold text-text-primary">{s.approvalTitle}</p>
        <p className="mt-0.5 text-xs text-text-secondary">
          {item.tool} · {item.action}
        </p>
        {item.reason !== '' && <p className="mt-0.5 text-xs text-text-tertiary">{item.reason}</p>}
        {item.resolved === undefined ? (
          <div className="mt-2 flex gap-2">
            <Button size="sm" onClick={() => onApproval?.(true)}>
              {s.approve}
            </Button>
            <Button variant="secondary" size="sm" onClick={() => onApproval?.(false)}>
              {s.deny}
            </Button>
          </div>
        ) : (
          <p className="mt-1.5 text-xs text-text-tertiary">
            {item.resolved === 'approved' ? s.approved : s.denied}
          </p>
        )}
      </div>
    );
  }

  return <ErrorState compact description={item.message} />;
}

/** Single unified diff: removed lines (red) directly above their replacement
 * (teal), unchanged lines plain — instead of two separate scrollable
 * Before/After panels. Text keeps the theme's own text-secondary color (never
 * a tinted one) so contrast stays whatever that token already guarantees in
 * both light and dark mode; only a light background tint carries the color. */
function DiffView({ before, after }: { before: string; after: string }) {
  const lines = useMemo(() => diffLines(before, after), [before, after]);
  return (
    <div className="max-h-80 overflow-auto py-1 font-mono text-[11px] leading-5">
      {/* `w-max min-w-full` is what makes each row's tint span the WHOLE line.
          These rows are block children of a horizontally scrolling box, so they
          size to its VISIBLE width, not its scroll width — a long line painted
          its highlight only as far as the viewport, so the color visibly cut
          off mid-row once scrolled. w-max grows this track to the widest line;
          min-w-full keeps short lines full-bleed. */}
      <div className="w-max min-w-full">
      {lines.map((line, index) => (
        <div
          key={index}
          className={`whitespace-pre px-3 text-text-secondary ${
            line.kind === 'removed'
              ? 'bg-danger/15'
              : line.kind === 'added'
                ? 'bg-accent/15'
                : ''
          }`}
        >
          {line.text === '' ? ' ' : line.text}
        </div>
      ))}
      </div>
    </div>
  );
}

function CodeDisclosure({ label, text }: { label: string; text: string }) {
  return (
    <div className="border-b border-border-subtle last:border-b-0">
      <p className="px-3 py-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
        {label}
      </p>
      <pre className="max-h-80 overflow-auto whitespace-pre p-3 pt-1 font-mono text-[11px] leading-5 text-text-secondary">
        <code>{text}</code>
      </pre>
    </div>
  );
}
