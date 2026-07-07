'use client';
// Toggle switch — a styled native checkbox (keyboard/a11y for free), tokens
// only. The shared on/off primitive for Settings fields and Skills toggles.
export function Switch({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <label
      className={`inline-flex select-none items-center gap-2 ${disabled ? 'opacity-50' : 'cursor-pointer'}`}
    >
      <input
        type="checkbox"
        role="switch"
        className="peer sr-only"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span
        aria-hidden
        className={`relative h-[18px] w-[32px] rounded-full transition-colors peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-accent ${
          checked ? 'bg-accent' : 'bg-stroke-secondary'
        }`}
      >
        <span
          className={`absolute top-[2px] h-[14px] w-[14px] rounded-full bg-on-accent transition-transform ${
            checked ? 'translate-x-[16px]' : 'translate-x-[2px]'
          }`}
        />
      </span>
      {label !== undefined && <span className="text-xs text-text-secondary">{label}</span>}
    </label>
  );
}
