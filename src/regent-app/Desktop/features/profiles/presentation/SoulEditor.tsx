'use client';
// The SOUL.md editor: persona.get loads into a monospace textarea, Save
// calls persona.set. A saved/dirty indicator tracks unsaved edits; errors
// render verbatim (never masked, per house rule).
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Button } from '@/shared/ui/Button';
import { t } from '@/shared/i18n/t';
import { usePersona } from '@/features/profiles/viewmodels/usePersona';

export function SoulEditor() {
  const s = t().profiles;
  const { content, dirty, loading, saving, error, setContent, save } = usePersona();

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-baseline justify-between gap-3">
        <div>
          <h2 className="text-xs font-semibold uppercase tracking-[0.08em] text-text-tertiary">{s.soulTitle}</h2>
          <p className="mt-0.5 text-xs text-text-tertiary">{s.soulDesc}</p>
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <span className="text-xs text-text-tertiary">{dirty ? s.dirty : s.saved}</span>
          <Button size="sm" disabled={!dirty || saving} onClick={save}>
            {saving ? <Loader /> : s.save}
          </Button>
        </div>
      </div>

      {error !== undefined && (
        <div className="mt-2">
          <ErrorState compact description={error} />
        </div>
      )}

      {loading ? (
        <div className="mt-6 flex justify-center">
          <Loader />
        </div>
      ) : (
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder={s.placeholder}
          aria-label={s.soulLabel}
          spellCheck={false}
          className="mt-3 min-h-40 w-full resize-y rounded-md bg-hover p-4 font-mono text-sm leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
        />
      )}
    </section>
  );
}
