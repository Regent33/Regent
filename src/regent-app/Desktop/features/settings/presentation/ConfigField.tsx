'use client';
// Binds one dotted config path to a control inside a FieldRow. Toggle writes
// immediately (Switch); Number/Text carry a draft and write on Apply; Select
// writes on change. Every write flows through the shared useConfig engine, so
// config.set validation errors surface verbatim (rendered by the parent
// section, once, from cfg.writeError). Render these only once cfg has loaded —
// the draft fields seed their value at mount (see primitives).
import { Switch } from '@/shared/ui/Switch';
import { FieldRow, NumberField, SelectField, TextField } from '@/features/settings/presentation/primitives';
import type { ConfigState } from '@/features/settings/viewmodels/useConfig';

export type ConfigControl =
  | { readonly kind: 'toggle' }
  | {
      readonly kind: 'number';
      readonly min?: number;
      readonly max?: number;
      readonly step?: number;
      readonly placeholder?: string;
    }
  | { readonly kind: 'text'; readonly placeholder?: string }
  | { readonly kind: 'select'; readonly options: readonly { readonly value: string; readonly label: string }[] };

export function ConfigField({
  cfg,
  path,
  label,
  description,
  applyLabel,
  control,
}: {
  cfg: ConfigState;
  path: string;
  label: string;
  description?: string;
  /** Apply-button copy for the Number/Text controls. */
  applyLabel: string;
  control: ConfigControl;
}) {
  const value = cfg.get(path);
  const saving = cfg.savingPath === path;

  if (control.kind === 'toggle') {
    return (
      <FieldRow
        label={label}
        description={description}
        control={
          <div className="sm:text-right">
            <Switch
              label={label}
              checked={value === true}
              disabled={saving}
              onChange={(next) => cfg.set(path, next)}
            />
          </div>
        }
      />
    );
  }

  if (control.kind === 'number') {
    return (
      <FieldRow
        label={label}
        description={description}
        control={
          <NumberField
            label={label}
            value={typeof value === 'number' ? value : undefined}
            placeholder={control.placeholder}
            min={control.min}
            max={control.max}
            step={control.step}
            applyLabel={applyLabel}
            applying={saving}
            onApply={(next) => cfg.set(path, next)}
          />
        }
      />
    );
  }

  if (control.kind === 'select') {
    return (
      <FieldRow
        label={label}
        description={description}
        control={
          <SelectField
            label={label}
            value={typeof value === 'string' ? value : ''}
            options={control.options}
            disabled={saving}
            onChange={(next) => cfg.set(path, next)}
          />
        }
      />
    );
  }

  return (
    <FieldRow
      label={label}
      description={description}
      control={
        <TextField
          label={label}
          value={typeof value === 'string' ? value : ''}
          placeholder={control.placeholder}
          applyLabel={applyLabel}
          applying={saving}
          onApply={(next) => cfg.set(path, next)}
        />
      }
    />
  );
}
