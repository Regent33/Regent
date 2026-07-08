// One transcript row — user bubbles are a quiet right-aligned block,
// assistant replies flat (flat-not-boxed), thinking/tool rows quiet activity
// lines (Hermes-style per-turn structure), approvals an actionable card,
// errors verbatim.
import { Button } from '@/shared/ui/Button';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import { Markdown } from '@/shared/ui/Markdown';
import { ChevronDownIcon, ErrorIcon, WrenchIcon } from '@/shared/ui/icons';
import { t } from '@/shared/i18n/t';
import type { TranscriptItem } from '@/shared/kernel/transcript';

export interface MessageRowProps {
  item: TranscriptItem;
  onApproval?: (approved: boolean) => void;
}

export function MessageRow({ item, onApproval }: MessageRowProps) {
  const s = t().chat.transcript;

  if (item.kind === 'user') {
    return (
      <div className="flex justify-end">
        <p className="max-w-[70%] whitespace-pre-wrap break-words rounded-[6px] bg-hover px-3 py-2 text-sm text-text-primary">
          {item.text}
        </p>
      </div>
    );
  }

  if (item.kind === 'assistant') {
    return (
      <div>
        <Markdown text={item.text} />
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
    return (
      <p className="flex items-center gap-1.5 text-xs text-text-tertiary">
        <WrenchIcon className="size-3.5 shrink-0" />
        <span className="truncate">{item.name}</span>
        {!item.done && <Loader />}
        {item.done && item.isError === true && <ErrorIcon className="size-3.5 shrink-0 text-danger" />}
      </p>
    );
  }

  if (item.kind === 'approval') {
    return (
      <div className="rounded-[6px] bg-hover px-3 py-2.5">
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
