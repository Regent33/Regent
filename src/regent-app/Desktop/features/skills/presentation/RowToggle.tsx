'use client';
// The row toggle for Skills & Tools — a real <button role="switch"> with
// aria-pressed, not the checkbox-based shared Switch, per this surface's
// design spec. Same pill visuals/tokens as Switch so it still reads as one
// on/off control language across the app.
export function RowToggle({
  checked,
  onToggle,
  label,
  disabled,
  title,
}: {
  checked: boolean;
  onToggle?: (next: boolean) => void;
  label: string;
  disabled?: boolean;
  title?: string;
}) {
  const inert = disabled === true || onToggle === undefined;
  return (
    <button
      type="button"
      role="switch"
      aria-pressed={checked}
      aria-checked={checked}
      aria-label={label}
      title={title}
      disabled={inert}
      onClick={() => onToggle?.(!checked)}
      className={`relative h-[18px] w-[32px] shrink-0 rounded-full p-0 transition-colors disabled:opacity-50 ${
        inert ? '' : 'cursor-pointer'
      } ${checked ? 'bg-accent' : 'bg-stroke-secondary'}`}
    >
      <span
        aria-hidden
        className={`absolute left-0 top-0.5 h-3.5 w-3.5 rounded-full bg-on-accent transition-transform ${
          checked ? 'translate-x-[16px]' : 'translate-x-[2px]'
        }`}
      />
    </button>
  );
}
