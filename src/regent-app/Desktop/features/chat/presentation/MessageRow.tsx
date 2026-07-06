// One transcript row — user bubbles are a quiet right-aligned block,
// assistant replies are flat (flat-not-boxed), errors surface verbatim.
import { ErrorState } from '@/shared/ui/ErrorState';
import { Loader } from '@/shared/ui/Loader';
import type { TranscriptItem } from '@/features/chat/domain/transcript';

export function MessageRow({ item }: { item: TranscriptItem }) {
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
      <p className="whitespace-pre-wrap break-words text-sm text-text-primary">
        {item.text}
        {item.streaming && <Loader className="ml-1.5 inline-flex align-middle" />}
      </p>
    );
  }

  return <ErrorState compact description={item.message} />;
}
