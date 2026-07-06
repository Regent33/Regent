// Quiet centered empty block — don't hand-roll centered empties per surface.
import type { ReactNode } from 'react';

export interface EmptyStateProps {
  icon?: ReactNode;
  title: ReactNode;
  hint?: ReactNode;
}

export function EmptyState({ icon, title, hint }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center gap-1.5 p-6 text-center">
      {icon !== undefined && <span className="text-text-tertiary">{icon}</span>}
      <p className="text-sm text-text-secondary">{title}</p>
      {hint !== undefined && <p className="text-xs text-text-tertiary">{hint}</p>}
    </div>
  );
}
