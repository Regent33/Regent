'use client';
// Skills & Tools — Hermes full-width grouped layout: top search field,
// Skills/Toolsets tab row, a category-chip row (name + count), then grouped
// sections per category (uppercase heading; name+description left, toggle
// right). Replaces the old master-detail split — there's no side detail pane.
//
// Wire shapes (checked against admin_ops.rs before writing this view):
//   skills.list  -> {name, description, tags: string[], archived?: bool}
//   tools.list   -> {name, description, toolset?: string, enabled: bool}
// Skills carry no single category field (`tags` is a free-form, possibly
// multi-valued list) so they render as one "All" group; the chip row still
// filters by tag. Tools carry a real (optional) `toolset` field, so Toolsets
// groups truly by category, with a fallback "Other" bucket for untagged tools.
import { useMemo, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { SearchField } from '@/shared/ui/SearchField';
import { useSkillsList } from '@/features/skills/viewmodels/useSkillsList';
import { useToolsList } from '@/features/skills/viewmodels/useToolsList';
import { CategoryChips } from '@/features/skills/presentation/CategoryChips';
import { GroupedRows, type GroupedItem } from '@/features/skills/presentation/GroupedRows';
import { RowToggle } from '@/features/skills/presentation/RowToggle';

type Tab = 'skills' | 'toolsets';

function matches(query: string, name: string, description?: string): boolean {
  const q = query.trim().toLowerCase();
  return q === '' || `${name} ${description ?? ''}`.toLowerCase().includes(q);
}

export function SkillsView() {
  const s = t().skills;
  const skills = useSkillsList();
  const tools = useToolsList();
  const [tab, setTab] = useState<Tab>('skills');
  const [query, setQuery] = useState('');
  const [chip, setChip] = useState<string>();

  const tabClass = (own: Tab) =>
    `rounded-[6px] px-2.5 py-1 text-xs font-medium transition-colors ${
      tab === own ? 'bg-hover text-text-primary' : 'text-text-tertiary hover:text-text-secondary'
    }`;

  // Skills: no real category field — every visible skill lands in the single
  // `s.chipAll` group; the chip row filters by tag same as before.
  const skillChipCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const skill of skills.skills) {
      for (const tag of skill.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
    return counts;
  }, [skills.skills]);

  const skillItems: GroupedItem[] = useMemo(
    () =>
      skills.skills
        .filter(
          (skill) =>
            matches(query, skill.name, skill.description) && (chip === undefined || skill.tags.includes(chip)),
        )
        .map((skill) => ({
          key: skill.name,
          name: skill.name,
          description: skill.description,
          // Hermes-style: the first tag is the section heading; untagged
          // skills bucket under "Other" (sorted last).
          category: skill.tags[0] ?? s.groupOther,
          dimmed: skill.archived,
          toggle: (
            <RowToggle
              checked={!skill.archived}
              onToggle={(on) => skills.setEnabled(skill.name, on)}
              label={skill.name}
            />
          ),
        })),
    [skills, query, chip, s.chipAll],
  );

  // Toolsets: `toolset` is a genuine (optional) category field — group by it,
  // with untagged tools falling into `s.groupOther`.
  const toolChipCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const tool of tools.tools) {
      const key = tool.toolset ?? s.groupOther;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  }, [tools.tools, s.groupOther]);

  const toolItems: GroupedItem[] = useMemo(
    () =>
      tools.tools
        .filter((tool) => {
          const category = tool.toolset ?? s.groupOther;
          return matches(query, tool.name, tool.description) && (chip === undefined || category === chip);
        })
        .map((tool) => ({
          key: tool.name,
          name: tool.name,
          description: tool.description,
          category: tool.toolset ?? s.groupOther,
          dimmed: !tool.enabled,
          // ponytail: display-only — no tool-toggle RPC exists; config
          // `tools.disabled` owns it. Make interactive when config.set lands.
          toggle: (
            <RowToggle checked={tool.enabled} label={tool.name} title={s.toolConfigManaged} disabled />
          ),
        })),
    [tools, query, chip, s.groupOther, s.toolConfigManaged],
  );

  const active = tab === 'skills' ? skills : tools;
  const items = tab === 'skills' ? skillItems : toolItems;
  const chipCounts = tab === 'skills' ? skillChipCounts : toolChipCounts;
  const total = tab === 'skills' ? skills.skills.length : tools.tools.length;
  const emptyLabel = tab === 'skills' ? s.skillsEmpty : s.toolsEmpty;

  return (
    <div className="flex h-full flex-col">
      {/* pr-12 keeps the right-aligned tabs clear of the overlay's close (×). */}
      <div className="flex items-center gap-3 border-b border-stroke-tertiary py-2 pl-3 pr-12">
        <SearchField
          label={s.searchLabel}
          placeholder={s.searchPlaceholder}
          className="flex-1"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <div className="flex gap-1">
          <button
            type="button"
            className={tabClass('skills')}
            onClick={() => {
              setTab('skills');
              setChip(undefined);
            }}
          >
            {s.skillsTitle} ({skills.skills.length})
          </button>
          <button
            type="button"
            className={tabClass('toolsets')}
            onClick={() => {
              setTab('toolsets');
              setChip(undefined);
            }}
          >
            {s.toolsetsTitle} ({tools.tools.length})
          </button>
        </div>
      </div>

      <CategoryChips counts={chipCounts} chip={chip} onChip={setChip} allLabel={s.chipAll} total={total} />

      <div className="min-h-0 flex-1 overflow-y-auto">
        {active.loading && (
          <div className="flex justify-center py-6">
            <Loader />
          </div>
        )}
        {active.error !== undefined && <ErrorState compact description={active.error} />}
        {!active.loading && active.error === undefined && (
          <GroupedRows items={items} emptyLabel={emptyLabel} otherLast={s.groupOther} />
        )}
      </div>
    </div>
  );
}
