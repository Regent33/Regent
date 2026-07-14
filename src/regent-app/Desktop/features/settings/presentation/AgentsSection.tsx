'use client';
// Agents — the full agent editor the CLI's `agents edit` offers, as a page:
// pick an agent on the left rail (or create one), edit every stored field
// (name is the identity; description, system prompt, model, tools), save via
// agents.set, delete via agents.remove. Skills-per-agent isn't a stored field
// yet (see docs/plans/bug-backlog) — the form edits exactly what dispatch reads.
import { useState } from 'react';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { type AgentDef, useAgents } from '@/features/settings/viewmodels/useAgents';

const EMPTY: AgentDef = { name: '', description: '', system_prompt: '', model: '', tools: '' };

export function AgentsSection() {
  const s = t().settings.agents;
  const vm = useAgents();
  const [selected, setSelected] = useState<string | null>(null);
  const [draft, setDraft] = useState<AgentDef | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const open = (a: AgentDef) => {
    setSelected(a.name);
    setDraft({ ...a, model: a.model ?? '', tools: a.tools ?? '' });
    setConfirmDelete(false);
  };
  const openNew = () => {
    setSelected(null);
    setDraft({ ...EMPTY });
    setConfirmDelete(false);
  };
  const set = (patch: Partial<AgentDef>) => setDraft((d) => (d ? { ...d, ...patch } : d));
  const creating = draft !== null && selected === null;
  const valid = draft !== null && draft.name.trim() !== '' && draft.description.trim() !== '';

  return (
    <Section title={s.title} description={s.description}>
      {vm.loading && <Loader />}
      {vm.error !== undefined && <ErrorState compact description={vm.error} />}
      {!vm.loading && (
        <div className="flex gap-4">
          <div className="w-44 shrink-0">
            {vm.agents.map((a) => (
              <button
                key={a.name}
                type="button"
                onClick={() => open(a)}
                className={`block w-full truncate rounded px-2.5 py-1.5 text-left text-sm ${
                  selected === a.name ? 'bg-surface-tertiary text-text-primary' : 'text-text-secondary hover:bg-surface-secondary'
                }`}
              >
                {a.name}
              </button>
            ))}
            {vm.agents.length === 0 && <p className="px-2.5 py-1.5 text-xs text-text-tertiary">{s.empty}</p>}
            <button
              type="button"
              onClick={openNew}
              className="mt-2 block w-full rounded px-2.5 py-1.5 text-left text-sm text-accent hover:bg-surface-secondary"
            >
              {s.newAgent}
            </button>
          </div>

          {draft === null ? (
            <p className="pt-1.5 text-sm text-text-tertiary">{s.pickHint}</p>
          ) : (
            <div className="min-w-0 flex-1 space-y-3">
              <Labeled label={s.nameLabel} hint={creating ? s.nameHint : s.nameLockedHint}>
                <input
                  className="w-full rounded border border-stroke-tertiary bg-surface-primary px-2 py-1.5 text-sm text-text-primary disabled:opacity-60"
                  value={draft.name}
                  disabled={!creating}
                  onChange={(e) => set({ name: e.target.value })}
                />
              </Labeled>
              <Labeled label={s.descriptionLabel} hint={s.descriptionHint}>
                <input
                  className="w-full rounded border border-stroke-tertiary bg-surface-primary px-2 py-1.5 text-sm text-text-primary"
                  value={draft.description}
                  onChange={(e) => set({ description: e.target.value })}
                />
              </Labeled>
              <Labeled label={s.promptLabel} hint={s.promptHint}>
                <textarea
                  className="min-h-40 w-full rounded border border-stroke-tertiary bg-surface-primary px-2 py-1.5 font-mono text-xs text-text-primary"
                  value={draft.system_prompt}
                  onChange={(e) => set({ system_prompt: e.target.value })}
                />
              </Labeled>
              <Labeled label={s.modelLabel} hint={s.modelHint}>
                <input
                  className="w-full rounded border border-stroke-tertiary bg-surface-primary px-2 py-1.5 text-sm text-text-primary"
                  value={draft.model ?? ''}
                  placeholder={s.modelPlaceholder}
                  onChange={(e) => set({ model: e.target.value })}
                />
              </Labeled>
              <Labeled label={s.toolsLabel} hint={s.toolsHint}>
                <input
                  className="w-full rounded border border-stroke-tertiary bg-surface-primary px-2 py-1.5 text-sm text-text-primary"
                  value={draft.tools ?? ''}
                  placeholder={s.toolsPlaceholder}
                  onChange={(e) => set({ tools: e.target.value })}
                />
              </Labeled>

              <div className="flex items-center gap-2 pt-1">
                <button
                  type="button"
                  disabled={!valid || vm.saving}
                  onClick={() => {
                    void vm.save(draft).then((ok) => {
                      if (ok && creating) setSelected(draft.name);
                    });
                  }}
                  className="rounded bg-accent px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
                >
                  {vm.saving ? s.saving : s.save}
                </button>
                {!creating && (
                  <button
                    type="button"
                    onClick={() => {
                      if (!confirmDelete) {
                        setConfirmDelete(true);
                        return;
                      }
                      void vm.remove(draft.name).then((ok) => {
                        if (ok) {
                          setDraft(null);
                          setSelected(null);
                        }
                      });
                    }}
                    className="rounded px-3 py-1.5 text-sm text-danger hover:bg-surface-secondary"
                  >
                    {confirmDelete ? s.deleteConfirm : s.delete}
                  </button>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </Section>
  );
}

function Labeled({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="text-sm font-medium text-text-primary">{label}</span>
      {hint !== undefined && <span className="ml-2 text-xs text-text-tertiary">{hint}</span>}
      <div className="mt-1">{children}</div>
    </label>
  );
}
