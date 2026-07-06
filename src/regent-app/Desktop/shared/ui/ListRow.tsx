// Flat label/description/trailing row for rails, lists, and settings.
// No per-row borders — grouping comes from spacing (flat-not-boxed).
import type { ButtonHTMLAttributes, ReactNode } from 'react';

export interface ListRowProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: ReactNode;
  label: string;
  description?: string;
  trailing?: ReactNode;
  active?: boolean;
}

export function ListRow({
  icon,
  label,
  description,
  trailing,
  active = false,
  className = '',
  ...props
}: ListRowProps) {
  return (
    <button
      type="button"
      className={`flex w-full cursor-pointer items-center gap-2.5 rounded-[4px] px-2.5 py-1.5 text-left transition-colors duration-100 ${
        active ? 'bg-hover text-text-primary' : 'text-text-secondary hover:bg-hover hover:text-text-primary'
      } ${className}`}
      {...props}
    >
      {icon !== undefined && <span className="shrink-0 text-text-tertiary">{icon}</span>}
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{label}</span>
        {description !== undefined && (
          <span className="block truncate text-xs text-text-tertiary">{description}</span>
        )}
      </span>
      {trailing !== undefined && <span className="shrink-0 text-xs text-text-tertiary">{trailing}</span>}
    </button>
  );
}
