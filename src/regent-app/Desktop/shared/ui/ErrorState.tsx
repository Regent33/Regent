// The canonical error block. Provider/deacon failures render their message
// VERBATIM here (401/402/429 included) — never truncated, never masked.
import type { ReactNode } from 'react';
import { t } from '@/shared/i18n/t';
import { ErrorIcon } from '@/shared/ui/icons';

export interface ErrorStateProps {
  title?: ReactNode;
  description: ReactNode;
  compact?: boolean;
}

export function ErrorState({ title, description, compact = false }: ErrorStateProps) {
  if (compact) {
    return (
      <p role="alert" className="flex items-start gap-1.5 px-2.5 py-1 text-xs text-danger">
        <ErrorIcon className="mt-0.5 size-3.5 shrink-0" />
        <span className="min-w-0 break-words">{description}</span>
      </p>
    );
  }
  return (
    <div role="alert" className="flex flex-col items-center gap-2 p-6 text-center">
      <ErrorIcon className="size-6 text-danger" />
      <p className="text-sm font-semibold text-text-primary">{title ?? t().ui.errorTitle}</p>
      <p className="max-w-md break-words text-sm text-text-secondary">{description}</p>
    </div>
  );
}
