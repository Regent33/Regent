import type { TranscriptItem } from '@/shared/kernel/transcript';
import type { TurnActivity } from '@/shared/state/turnActivity';

export const chatBusy = (localBusy: boolean, activity: TurnActivity): boolean =>
  localBusy || activity === 'running';

/** Restore an error that completed while this route was unmounted, once. */
export function withTurnError(
  items: readonly TranscriptItem[],
  error: string | undefined,
): readonly TranscriptItem[] {
  if (error === undefined) return items;
  if (items.some((item) => item.kind === 'error' && item.message === error)) return items;
  return [...items, { kind: 'error', message: error }];
}
