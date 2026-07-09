'use client';
// Shared key-picker control: a Key <select> bound to ONE row's `key_slot` —
// the main model row and each fallback row pick their key independently
// (per-ref key_slot in agents_defaults), so the same provider+model can ride
// two different keys. Rendered only when more than one slot is stored.
// Slot 1 (the base key) is the unpinned default.
import { SelectField } from '@/features/settings/presentation/primitives';
import type { KeySlot } from '@/features/settings/viewmodels/useMainModels';

export function KeyPickerField({
  slots,
  value,
  onSelect,
  label,
}: {
  readonly slots: readonly KeySlot[];
  /** The row's pinned slot; undefined = slot 1 (base key). */
  readonly value?: number;
  readonly onSelect: (slot: number) => void;
  readonly label: string;
}) {
  if (slots.length <= 1) return null;
  return (
    <SelectField
      label={label}
      value={String(value ?? 1)}
      options={slots.map(({ slot, masked }) => ({
        value: String(slot),
        label: `${label} ${slot}${masked !== undefined ? ` ${masked}` : ''}`,
      }))}
      onChange={(next) => onSelect(Number(next))}
    />
  );
}
