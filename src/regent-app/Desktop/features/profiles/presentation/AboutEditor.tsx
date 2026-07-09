'use client';
// The About section — the five structured user-profile facets the deacon
// renders into every prompt (store persona_block: about.identity/preferences/
// habits/constraints/goals). Each facet is its own editable, resizable field
// with its own save; SOUL stays a separate section above.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Button } from '@/shared/ui/Button';
import { t } from '@/shared/i18n/t';
import { usePersona } from '@/features/profiles/viewmodels/usePersona';

const FACETS = ['identity', 'preferences', 'habits', 'constraints', 'goals'] as const;
type Facet = (typeof FACETS)[number];

function FacetField({ facet }: { facet: Facet }) {
  const s = t().profiles.about;
  const { content, dirty, loading, saving, error, setContent, save } = usePersona(`about.${facet}`);

  return (
    <div className="mt-4 first:mt-2">
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-xs font-semibold uppercase tracking-[0.08em] text-text-tertiary">
          {s.facets[facet]}
        </h3>
        <div className="flex shrink-0 items-center gap-3">
          {dirty && <span className="text-xs text-text-tertiary">{t().profiles.dirty}</span>}
          <Button size="sm" disabled={!dirty || saving} onClick={save}>
            {saving ? <Loader /> : t().profiles.save}
          </Button>
        </div>
      </div>
      {error !== undefined && (
        <div className="mt-1">
          <ErrorState compact description={error} />
        </div>
      )}
      {loading ? (
        <Loader className="mt-2" />
      ) : (
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder={s.placeholder}
          aria-label={s.facets[facet]}
          spellCheck={false}
          className="mt-1.5 min-h-20 w-full resize-y rounded-md bg-hover p-3 text-sm leading-relaxed text-text-primary outline-none placeholder:text-text-tertiary"
        />
      )}
    </div>
  );
}

export function AboutEditor() {
  const s = t().profiles.about;
  return (
    <section>
      <h2 className="text-xs font-semibold uppercase tracking-[0.08em] text-text-tertiary">{s.title}</h2>
      <p className="mt-0.5 text-xs text-text-tertiary">{s.desc}</p>
      {FACETS.map((facet) => (
        <FacetField key={facet} facet={facet} />
      ))}
    </section>
  );
}
