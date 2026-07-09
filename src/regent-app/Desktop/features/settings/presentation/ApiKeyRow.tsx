'use client';
// One env-key row. Set: masked preview + Replace/Remove. Unset (or Replacing):
// a password input + Save. The raw key is never rendered — only the deacon's
// masked preview. Save clears the draft immediately so it can't linger.
import { useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { Loader } from '@/shared/ui/Loader';
import { t } from '@/shared/i18n/t';
import { FieldRow, SelectField, TextInput } from '@/features/settings/presentation/primitives';
import type { EnvKey } from '@/features/settings/viewmodels/useApiKeys';

export interface KeySlot {
  readonly slot: number;
  readonly masked?: string;
}

export function ApiKeyRow({
  entry,
  saving,
  onSave,
  onRemove,
  addSlotName,
  slots,
  onActivate,
}: {
  entry: EnvKey;
  saving: boolean;
  onSave: (name: string, value: string) => void;
  onRemove: (name: string) => void;
  /** Next free numbered slot (`<BASE>_2`…) — enables the "Add key" affordance
   * on a set base row so one provider can hold multiple keys. */
  addSlotName?: string;
  /** All stored slots for this base (slot 1 = the base). >1 shows the
   * active-key dropdown; picking slot N swaps it into the base. */
  slots?: readonly KeySlot[];
  onActivate?: (slot: number) => void;
}) {
  const s = t().settings.apiKeys;
  const [replacing, setReplacing] = useState(false);
  const [addingSlot, setAddingSlot] = useState(false);
  const [draft, setDraft] = useState('');

  const commit = () => {
    if (draft.trim() === '') return;
    onSave(addingSlot && addSlotName !== undefined ? addSlotName : entry.name, draft.trim());
    setDraft('');
    setReplacing(false);
    setAddingSlot(false);
  };

  const editing = !entry.set || replacing || addingSlot;

  const control = editing ? (
    <div className="flex items-center gap-2">
      <TextInput
        label={entry.label}
        type="password"
        value={draft}
        placeholder={s.placeholder}
        disabled={saving}
        onChange={setDraft}
        onKeyDown={(e) => {
          if (e.key === 'Enter') commit();
        }}
      />
      <Button size="sm" onClick={commit} disabled={draft.trim() === '' || saving}>
        {saving ? <Loader /> : s.save}
      </Button>
      {entry.set && (
        <Button
          size="sm"
          variant="ghost"
          onClick={() => {
            setReplacing(false);
            setAddingSlot(false);
            setDraft('');
          }}
        >
          {s.cancel}
        </Button>
      )}
    </div>
  ) : (
    <div className="flex items-center gap-2">
      {slots !== undefined && slots.length > 1 && onActivate !== undefined ? (
        <span className="w-36 shrink-0">
          <SelectField
            label={s.activeKey}
            value="1"
            options={slots.map(({ slot, masked }) => ({
              value: String(slot),
              label: `${s.keyOption} ${slot}${masked !== undefined ? ` ${masked}` : ''}`,
            }))}
            disabled={saving}
            onChange={(next) => {
              const slot = Number(next);
              if (slot > 1) onActivate(slot);
            }}
          />
        </span>
      ) : (
        <code className="min-w-0 truncate text-xs text-text-tertiary">{entry.masked ?? s.set}</code>
      )}
      <Button size="sm" variant="secondary" onClick={() => setReplacing(true)} disabled={saving}>
        {s.replace}
      </Button>
      <Button size="sm" variant="ghost" onClick={() => onRemove(entry.name)} disabled={saving}>
        {saving ? <Loader /> : s.remove}
      </Button>
      {addSlotName !== undefined && (
        <Button
          size="sm"
          variant="ghost"
          aria-label={s.addKey}
          title={s.addKey}
          className="shrink-0 px-1.5"
          onClick={() => setAddingSlot(true)}
          disabled={saving}
        >
          +
        </Button>
      )}
    </div>
  );

  return (
    <FieldRow label={entry.label} description={entry.set ? undefined : s.unset} control={control} />
  );
}
