'use client';
// Settings field kit (Hermes primitives.tsx equivalent): a Section wrapper,
// the label-left/control-right FieldRow grid (control vertically centered —
// the "centered Apply"), and a dirty-tracking TextField. Only the pieces the
// sections actually use — grow it when a section needs more.
import { type ReactNode, useState } from 'react';
import { Button } from '@/shared/ui/Button';

export function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="mx-auto w-full max-w-2xl p-6">
      <h2 className="text-lg font-semibold text-text-primary">{title}</h2>
      {description !== undefined && <p className="mt-1 text-xs text-text-tertiary">{description}</p>}
      <div className="mt-4">{children}</div>
    </div>
  );
}

export function FieldRow({
  label,
  description,
  control,
}: {
  label: string;
  description?: string;
  control: ReactNode;
}) {
  return (
    <div className="grid gap-3 border-b border-stroke-tertiary py-3 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_minmax(11rem,16rem)] sm:items-center">
      <div className="min-w-0">
        <p className="text-sm font-medium text-text-primary">{label}</p>
        {description !== undefined && <p className="mt-0.5 text-xs text-text-tertiary">{description}</p>}
      </div>
      <div className="min-w-0 sm:justify-self-end">{control}</div>
    </div>
  );
}

/** Free-text field with an Apply button that arms only when the value is
 * dirty vs `value` (the last saved state). Submit on Enter too. */
export function TextField({
  value,
  placeholder,
  applyLabel,
  applying,
  onApply,
  label,
}: {
  value: string;
  placeholder?: string;
  applyLabel: string;
  applying?: boolean;
  onApply: (next: string) => void;
  label: string;
}) {
  const [draft, setDraft] = useState(value);
  const dirty = draft !== value && draft.trim() !== '';
  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (dirty) onApply(draft.trim());
      }}
    >
      <input
        aria-label={label}
        className="w-full min-w-0 rounded-[6px] border border-stroke-secondary bg-bg px-2 py-1 text-sm text-text-primary outline-none placeholder:text-text-tertiary focus:border-accent"
        value={draft}
        placeholder={placeholder}
        onChange={(e) => setDraft(e.target.value)}
      />
      <Button size="sm" type="submit" disabled={!dirty || applying === true}>
        {applyLabel}
      </Button>
    </form>
  );
}
