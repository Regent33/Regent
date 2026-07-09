'use client';
// Shared key-picker control: a Key <select> that swaps a numbered stored key
// slot into a provider's active key (env.activate) — rendered only when more
// than one slot is stored for the provider. Used by both the main model row
// (MainModelPicker) and each fallback row (MainModelsSection) so every picker
// offers the same key choice.
import { SelectField } from '@/features/settings/presentation/primitives';
import type { KeySlot } from '@/features/settings/viewmodels/useMainModels';

export function KeyPickerField({
  slots,
  onActivate,
  label,
}: {
  readonly slots: readonly KeySlot[];
  readonly onActivate: (slot: number) => void;
  readonly label: string;
}) {
  if (slots.length <= 1) return null;
  return (
    <SelectField
      label={label}
      value="1"
      options={slots.map(({ slot, masked }) => ({
        value: String(slot),
        label: `${label} ${slot}${masked !== undefined ? ` ${masked}` : ''}`,
      }))}
      onChange={(next) => {
        const picked = Number(next);
        if (picked > 1) onActivate(picked);
      }}
    />
  );
}
