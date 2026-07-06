// The only search input — borderless, underline-on-focus (opts out of the
// global focus ring in favor of the underline affordance).
import type { InputHTMLAttributes } from 'react';
import { SearchIcon } from '@/shared/ui/icons';

export interface SearchFieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string;
}

export function SearchField({ label, className = '', ...props }: SearchFieldProps) {
  return (
    <div
      className={`flex items-center gap-2 border-b border-transparent px-1 py-1.5 text-text-tertiary transition-colors duration-100 focus-within:border-accent focus-within:text-text-secondary ${className}`}
    >
      <SearchIcon className="size-3.5 shrink-0" />
      <input
        type="search"
        aria-label={label}
        className="w-full bg-transparent text-sm text-text-primary outline-none placeholder:text-text-tertiary focus-visible:outline-none"
        {...props}
      />
    </div>
  );
}
