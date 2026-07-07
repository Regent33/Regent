'use client';
// Appearance section — the Light / Dark / System theme pick, wired to the
// theme store (which stamps <html data-theme> and persists the choice). A
// compact three-way segmented control sits in the FieldRow's control slot;
// picking a segment applies the theme instantly, no Apply button.
import { t } from '@/shared/i18n/t';
import { type ThemeMode, useTheme } from '@/shared/state/theme';
import { Section, FieldRow } from '@/features/settings/presentation/primitives';

export function AppearanceSection() {
  const s = t().settings.appearance;
  const { mode, setMode } = useTheme();
  const options: ReadonlyArray<{ id: ThemeMode; label: string }> = [
    { id: 'light', label: s.light },
    { id: 'dark', label: s.dark },
    { id: 'system', label: s.system },
  ];

  return (
    <Section title={s.title} description={s.description}>
      <FieldRow
        label={s.themeLabel}
        description={s.themeHint}
        control={
          <div
            role="radiogroup"
            aria-label={s.themeLabel}
            className="flex w-full rounded-[8px] border border-stroke-secondary bg-bg p-0.5"
          >
            {options.map((option) => {
              const active = mode === option.id;
              return (
                <button
                  key={option.id}
                  type="button"
                  role="radio"
                  aria-checked={active}
                  onClick={() => setMode(option.id)}
                  className={`flex-1 cursor-pointer rounded-[6px] px-2 py-1 text-xs font-medium transition-colors duration-100 ${
                    active
                      ? 'bg-accent text-on-accent'
                      : 'text-text-secondary hover:bg-hover hover:text-text-primary'
                  }`}
                >
                  {option.label}
                </button>
              );
            })}
          </div>
        }
      />
    </Section>
  );
}
