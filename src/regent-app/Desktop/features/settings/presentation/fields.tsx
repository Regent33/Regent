'use client';
// The settings input controls that share one chrome recipe: the dirty-tracking
// TextField/NumberField (Apply arms on change), the bare TextInput (commit
// lives elsewhere), and the native SelectField (no picker lib). Re-exported
// from primitives.tsx so call sites keep a single import path.
import { type KeyboardEvent, useState } from 'react';
import { Button } from '@/shared/ui/Button';
import { ChevronDownIcon } from '@/shared/ui/icons';

// Shared control chrome — one border/radius/focus recipe for the text, number,
// and select inputs so every settings field lines up (tokens only).
const CONTROL =
  'w-full min-w-0 rounded-[6px] border border-stroke-secondary bg-bg px-2 py-1 text-sm text-text-primary outline-none placeholder:text-text-tertiary focus:border-accent';

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
        className={CONTROL}
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

/** Bare controlled input (no Apply) — for controls whose commit lives
 * elsewhere (the Model picker's shared Apply, the API-key password field). */
export function TextInput({
  value,
  onChange,
  placeholder,
  label,
  type,
  disabled,
  onKeyDown,
  onBlur,
}: {
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  label: string;
  type?: 'text' | 'password';
  disabled?: boolean;
  onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void;
  onBlur?: () => void;
}) {
  return (
    <input
      aria-label={label}
      type={type ?? 'text'}
      className={CONTROL}
      value={value}
      placeholder={placeholder}
      disabled={disabled === true}
      autoComplete="off"
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={onKeyDown}
      onBlur={onBlur}
    />
  );
}

/** Numeric sibling of TextField: an Apply that arms only when the parsed value
 * is a real number AND differs from `value` (the last saved state). Submit on
 * Enter. Guard rendering until the stored value has loaded — like TextField,
 * the draft seeds once from `value` (matches the existing field pattern). */
export function NumberField({
  value,
  placeholder,
  applyLabel,
  applying,
  onApply,
  label,
  min,
  max,
  step,
}: {
  value?: number;
  placeholder?: string;
  applyLabel: string;
  applying?: boolean;
  onApply: (next: number) => void;
  label: string;
  min?: number;
  max?: number;
  step?: number;
}) {
  // Strip float noise (0.8500000000000001, 0.049999999…) before display/write.
  const initial = value === undefined ? '' : String(Number(value.toPrecision(12)));
  const [draft, setDraft] = useState(initial);
  const parsed = Number(Number(draft).toPrecision(12));
  const dirty = draft.trim() !== '' && draft !== initial && !Number.isNaN(parsed);

  // Custom stepper replacing the native spin buttons (which overflow the
  // rounded control chrome) — same step/min/max the native ones honored.
  const bump = (dir: 1 | -1) => {
    const base = draft.trim() === '' || Number.isNaN(parsed) ? (value ?? min ?? 0) : parsed;
    let next = Number((base + dir * (step ?? 1)).toPrecision(12));
    if (min !== undefined && next < min) next = min;
    if (max !== undefined && next > max) next = max;
    setDraft(String(next));
  };
  const STEP_BTN =
    'flex h-3 w-4 items-center justify-center text-text-tertiary hover:text-text-primary';

  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (dirty) onApply(parsed);
      }}
    >
      <span className="relative inline-flex w-full min-w-0 items-center">
        <input
          aria-label={label}
          type="number"
          className={`${CONTROL} pr-6 [appearance:textfield] [&::-webkit-inner-spin-button]:hidden [&::-webkit-outer-spin-button]:hidden`}
          value={draft}
          placeholder={placeholder}
          min={min}
          max={max}
          step={step}
          onChange={(e) => setDraft(e.target.value)}
        />
        <span className="absolute right-1 flex flex-col">
          <button type="button" tabIndex={-1} aria-label={`${label} +`} className={STEP_BTN} onClick={() => bump(1)}>
            <ChevronDownIcon className="size-3 rotate-180" />
          </button>
          <button type="button" tabIndex={-1} aria-label={`${label} −`} className={STEP_BTN} onClick={() => bump(-1)}>
            <ChevronDownIcon className="size-3" />
          </button>
        </span>
      </span>
      <Button size="sm" type="submit" disabled={!dirty || applying === true}>
        {applyLabel}
      </Button>
    </form>
  );
}

/** Native <select> styled to match the field kit (no picker lib). Writes on
 * change — the caller decides whether that hits config.set or local state. */
export function SelectField({
  value,
  options,
  onChange,
  disabled,
  label,
  placeholder,
}: {
  value: string;
  options: readonly { readonly value: string; readonly label: string }[];
  onChange: (next: string) => void;
  disabled?: boolean;
  label: string;
  placeholder?: string;
}) {
  // appearance-none kills the native glyph (invisible against the dark
  // tokens in WebView2); the token-tinted chevron overlay replaces it.
  return (
    <span className="relative inline-flex w-full min-w-0 items-center">
      <select
        aria-label={label}
        className={`${CONTROL} cursor-pointer appearance-none pr-7 disabled:cursor-default disabled:opacity-50`}
        value={value}
        disabled={disabled === true}
        onChange={(e) => onChange(e.target.value)}
      >
        {placeholder !== undefined && (
          <option value="" disabled>
            {placeholder}
          </option>
        )}
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
      <ChevronDownIcon className="pointer-events-none absolute right-2 size-3.5 text-text-secondary" />
    </span>
  );
}
