import type { InputHTMLAttributes } from "react";

export function Checkbox({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 rounded-lg px-2 py-2 transition-colors duration-150 hover:bg-hover">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="mt-0.5 h-4 w-4 accent-[var(--accent)]"
      />
      <span className="min-w-0">
        <span className="block text-sm text-text-primary">{label}</span>
        {hint && <span className="mt-0.5 block text-xs text-text-tertiary">{hint}</span>}
      </span>
    </label>
  );
}

export function TextInput({ className = "", ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={`w-full select-text rounded-lg border border-stroke-secondary bg-surface px-3 py-2 text-sm text-text-primary transition-colors duration-150 placeholder:text-text-tertiary focus:border-accent ${className}`}
      {...rest}
    />
  );
}
