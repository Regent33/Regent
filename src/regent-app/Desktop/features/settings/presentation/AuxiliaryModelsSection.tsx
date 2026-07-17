'use client';
// The learning-reviewer picker (config model.review): the model the background
// review loop runs on instead of inheriting each session's chat model. "Inherit
// chat model" clears it (writes null). Options come from model.list — the same
// catalog the composer picker offers — plus the stored value if it's a custom
// ref not in that list, so a hand-set model shows instead of resetting.
import { useEffect, useState } from 'react';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { t } from '@/shared/i18n/t';
import { FieldRow, SelectField } from '@/features/settings/presentation/primitives';
import type { ConfigState } from '@/features/settings/viewmodels/useConfig';

interface ModelRow {
  readonly id: string;
  readonly display_name: string;
}

// Sentinel for "no override" — writes null so the reviewer inherits chat.
const INHERIT = '__inherit__';

export function AuxiliaryModelsSection({ cfg }: { cfg: ConfigState }) {
  const s = t().settings.model;
  const [models, setModels] = useState<readonly ModelRow[]>([]);

  useEffect(() => {
    void deaconRequest<ModelRow[]>('model.list', {}).then((r) => {
      if (r.ok && Array.isArray(r.value)) setModels(r.value);
    });
  }, []);

  const current = cfg.get('model.review');
  const value = typeof current === 'string' && current !== '' ? current : INHERIT;
  const options = [
    { value: INHERIT, label: s.reviewInherit },
    // A stored ref that model.list doesn't carry (custom / failover id) stays
    // selectable instead of silently snapping back to Inherit.
    ...(value !== INHERIT && !models.some((m) => m.id === value) ? [{ value, label: value }] : []),
    ...models.map((m) => ({ value: m.id, label: m.display_name })),
  ];

  return (
    <FieldRow
      label={s.reviewLabel}
      description={s.reviewHint}
      control={
        <SelectField
          label={s.reviewLabel}
          value={value}
          options={options}
          disabled={cfg.savingPath === 'model.review'}
          onChange={(next) => cfg.set('model.review', next === INHERIT ? null : next)}
        />
      }
    />
  );
}
