'use client';
// Skills & Tools — Hermes IA: top search + Skills/Tools tabs + category
// chips (skill tags / toolsets) over toggleable rows, with the master-detail
// pane kept. Skill switches call skills.opt_in/opt_out (archived rows render
// dimmed but stay listed); tool switches only display config state — there
// is no tool-toggle RPC yet, config `tools.disabled` owns it.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { SearchField } from '@/shared/ui/SearchField';
import { useSkillsList } from '@/features/skills/viewmodels/useSkillsList';
import { useToolsList } from '@/features/skills/viewmodels/useToolsList';
import { SkillDetailPane } from '@/features/skills/presentation/SkillDetailPane';
import { ToolDetailPane } from '@/features/skills/presentation/ToolDetailPane';
import { SkillList, ToolList } from '@/features/skills/presentation/SkillsLists';

type Tab = 'skills' | 'tools';
type Selection = { kind: 'skill' | 'tool'; name: string } | undefined;

export function SkillsView() {
  const s = t().skills;
  const skills = useSkillsList();
  const tools = useToolsList();
  const [tab, setTab] = useState<Tab>('skills');
  const [query, setQuery] = useState('');
  const [chip, setChip] = useState<string>();
  const [selected, setSelected] = useState<Selection>();

  const selectedTool = selected?.kind === 'tool' ? tools.tools.find((tl) => tl.name === selected.name) : undefined;
  const active = tab === 'skills' ? skills : tools;
  const tabClass = (own: Tab) =>
    `rounded-[6px] px-2.5 py-1 text-xs font-medium transition-colors ${
      tab === own ? 'bg-hover text-text-primary' : 'text-text-tertiary hover:text-text-secondary'
    }`;

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-stroke-tertiary px-3 py-2">
        <SearchField
          label={s.searchLabel}
          placeholder={s.searchPlaceholder}
          className="flex-1"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="flex gap-1">
          <button type="button" className={tabClass('skills')} onClick={() => { setTab('skills'); setChip(undefined); }}>
            {s.skillsTitle} ({skills.skills.length})
          </button>
          <button type="button" className={tabClass('tools')} onClick={() => { setTab('tools'); setChip(undefined); }}>
            {s.toolsTitle} ({tools.tools.length})
          </button>
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        <nav className="w-[280px] shrink-0 overflow-y-auto border-r border-stroke-tertiary p-2">
          {active.loading && (
            <div className="flex justify-center py-2">
              <Loader />
            </div>
          )}
          {active.error !== undefined && <ErrorState compact description={active.error} />}
          {!active.loading && active.error === undefined && tab === 'skills' && (
            <SkillList
              skills={skills.skills}
              query={query}
              chip={chip}
              onChip={setChip}
              selected={selected?.kind === 'skill' ? selected.name : undefined}
              onSelect={(name) => setSelected({ kind: 'skill', name })}
              onToggle={skills.setEnabled}
            />
          )}
          {!active.loading && active.error === undefined && tab === 'tools' && (
            <ToolList
              tools={tools.tools}
              query={query}
              chip={chip}
              onChip={setChip}
              selected={selected?.kind === 'tool' ? selected.name : undefined}
              onSelect={(name) => setSelected({ kind: 'tool', name })}
            />
          )}
        </nav>

        <div className="min-w-0 flex-1 overflow-y-auto">
          {selected === undefined && (
            <div className="flex h-full items-center justify-center">
              <EmptyState title={s.selectHint} />
            </div>
          )}
          {selected?.kind === 'skill' && <SkillDetailPane name={selected.name} />}
          {selectedTool !== undefined && <ToolDetailPane tool={selectedTool} />}
        </div>
      </div>
    </div>
  );
}
