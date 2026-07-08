'use client';
// Settings field kit (Hermes primitives.tsx equivalent): a Section wrapper and
// the label-left/control-right FieldRow grid (control vertically centered — the
// "centered Apply"). The input controls (TextField/TextInput/NumberField/
// SelectField) live in fields.tsx and are re-exported here so every section
// keeps one import path.
import { type ReactNode } from 'react';

export { NumberField, SelectField, TextField, TextInput } from '@/features/settings/presentation/fields';

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
