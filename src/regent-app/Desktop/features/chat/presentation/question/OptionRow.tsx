// One answer row of a question card: number chip, label, optional description,
// and a marker that says what the question expects — a radio for single-select,
// a checkbox for multi-select, a pick-order badge for rank.
//
// The label is model-authored text and is rendered as TEXT, never markdown
// (§9 — this is the prompt-injection boundary). React escapes it; nothing here
// touches dangerouslySetInnerHTML.
//
// Not a <button>: the card body holds focus and names the highlighted row with
// aria-activedescendant, so rows carry radio/checkbox roles and aria-checked
// without competing for the tab stop.
import { CheckIcon } from '@/shared/ui/icons';

export interface OptionRowProps {
  /** DOM id — the card points aria-activedescendant at it. */
  id: string;
  /** 0-based; the chip shows index + 1, which is also its 1-9 shortcut. */
  index: number;
  label: string;
  description?: string;
  multi: boolean;
  checked: boolean;
  /** 1-based pick order, for `rank` questions only. */
  rank?: number;
  /** This row is under the keyboard cursor. */
  cursor: boolean;
  onSelect: () => void;
}

function Marker({ multi, checked, rank }: Pick<OptionRowProps, 'multi' | 'checked' | 'rank'>) {
  if (rank !== undefined) {
    return (
      <span className="flex size-5 shrink-0 items-center justify-center rounded-full bg-accent text-[11px] font-semibold text-on-accent">
        {rank}
      </span>
    );
  }
  const shape = multi ? 'rounded-[4px]' : 'rounded-full';
  return (
    <span
      className={`flex size-5 shrink-0 items-center justify-center border ${shape} ${
        checked ? 'border-accent bg-accent text-on-accent' : 'border-stroke-primary'
      }`}
    >
      {checked &&
        (multi ? <CheckIcon className="size-3" /> : <span className="size-2 rounded-full bg-on-accent" />)}
    </span>
  );
}

export function OptionRow({
  id,
  index,
  label,
  description,
  multi,
  checked,
  rank,
  cursor,
  onSelect,
}: OptionRowProps) {
  return (
    <div
      id={id}
      role={multi ? 'checkbox' : 'radio'}
      aria-checked={checked}
      onClick={onSelect}
      // min-h-11 = 44px, the minimum comfortable touch target.
      className={`flex min-h-11 cursor-pointer items-center gap-2.5 rounded-sm border-l-2 px-2.5 py-1.5 transition-colors duration-100 ${
        cursor ? 'border-accent bg-hover text-text-primary' : 'border-transparent text-text-secondary hover:bg-hover'
      }`}
    >
      <Marker multi={multi} checked={checked} rank={rank} />
      <span className="flex size-5 shrink-0 items-center justify-center rounded-sm bg-surface font-mono text-[11px] text-text-tertiary">
        {index + 1}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block break-words text-sm">{label}</span>
        {description !== undefined && description !== '' && (
          <span className="mt-0.5 block break-words text-xs text-text-tertiary">{description}</span>
        )}
      </span>
    </div>
  );
}
