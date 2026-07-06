'use client';
// Profiles — the SOUL editor: persona.get loads into a monospace textarea,
// Save calls persona.set. A saved/dirty indicator tracks unsaved edits;
// errors render verbatim (never masked, per house rule).
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Button } from '@/shared/ui/Button';
import { t } from '@/shared/i18n/t';
import { usePersona } from '@/features/profiles/viewmodels/usePersona';

export function ProfilesView() {
  const s = t().profiles;
  const { content, dirty, loading, saving, error, setContent, save } = usePersona();

  return (
    <div className="flex h-full flex-col p-6">
      <div className="flex items-center justify-between gap-3">
        <h1 className="text-lg font-semibold text-text-primary">{s.title}</h1>
        <div className="flex items-center gap-3">
          <span className="text-xs text-text-tertiary">{dirty ? s.dirty : s.saved}</span>
          <Button size="sm" disabled={!dirty || saving} onClick={save}>
            {saving ? <Loader /> : s.save}
          </Button>
        </div>
      </div>

      {error !== undefined && (
        <div className="mt-3">
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
          className="mt-4 min-h-0 flex-1 resize-none rounded-[6px] bg-hover p-4 font-mono text-sm leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
        />
      )}
    </div>
  );
}
