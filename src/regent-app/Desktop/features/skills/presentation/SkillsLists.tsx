'use client';
// The filterable row lists for SkillsView: category chips (skill tags /
// toolsets) with counts, then rows with a trailing Switch. Skill switches
// mutate (opt in/out); tool switches display config state only.
import { t } from '@/shared/i18n/t';
import { Switch } from '@/shared/ui/Switch';
import type { SkillRow } from '@/features/skills/viewmodels/useSkillsList';
import type { ToolRow } from '@/features/skills/viewmodels/useToolsList';

function matches(query: string, name: string, description?: string): boolean {
  const q = query.trim().toLowerCase();
  return q === '' || `${name} ${description ?? ''}`.toLowerCase().includes(q);
}

function Chips({
  counts,
  chip,
  onChip,
  allLabel,
  total,
}: {
  counts: ReadonlyMap<string, number>;
  chip?: string;
  onChip: (chip?: string) => void;
  allLabel: string;
  total: number;
}) {
  const chipClass = (own?: string) =>
    `rounded-full px-2 py-0.5 text-[11px] transition-colors ${
      chip === own ? 'bg-accent text-on-accent' : 'bg-hover text-text-secondary hover:text-text-primary'
    }`;
  return (
    <div className="flex flex-wrap gap-1 px-1 pb-2">
      <button type="button" className={chipClass(undefined)} onClick={() => onChip(undefined)}>
        {allLabel} {total}
      </button>
      {[...counts.entries()].sort().map(([name, count]) => (
        <button key={name} type="button" className={chipClass(name)} onClick={() => onChip(name)}>
          {name} {count}
        </button>
      ))}
    </div>
  );
}

function Row({
  name,
  description,
  dimmed,
  active,
  onSelect,
  trailing,
}: {
  name: string;
  description?: string;
  dimmed?: boolean;
  active: boolean;
  onSelect: () => void;
  trailing: React.ReactNode;
}) {
  return (
    <div
      className={`flex items-center gap-2 rounded-[6px] px-2.5 py-1.5 transition-colors ${
        active ? 'bg-hover' : 'hover:bg-hover'
      } ${dimmed === true ? 'opacity-50' : ''}`}
    >
      <button type="button" className="min-w-0 flex-1 text-left" onClick={onSelect}>
        <p className="truncate text-sm text-text-primary">{name}</p>
        {description !== undefined && <p className="truncate text-xs text-text-tertiary">{description}</p>}
      </button>
      {trailing}
    </div>
  );
}

export function SkillList({
  skills,
  query,
  chip,
  onChip,
  selected,
  onSelect,
  onToggle,
}: {
  skills: readonly SkillRow[];
  query: string;
  chip?: string;
  onChip: (chip?: string) => void;
  selected?: string;
  onSelect: (name: string) => void;
  onToggle: (name: string, enabled: boolean) => void;
}) {
  const s = t().skills;
  const counts = new Map<string, number>();
  for (const skill of skills) {
    for (const tag of skill.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
  }
  const rows = skills.filter(
    (skill) =>
      matches(query, skill.name, skill.description) && (chip === undefined || skill.tags.includes(chip)),
  );
  return (
    <>
      <Chips counts={counts} chip={chip} onChip={onChip} allLabel={s.chipAll} total={skills.length} />
      {rows.length === 0 && <p className="px-2.5 text-xs text-text-tertiary">{s.skillsEmpty}</p>}
      {rows.map((skill) => (
        <Row
          key={skill.name}
          name={skill.name}
          description={skill.description}
          dimmed={skill.archived}
          active={selected === skill.name}
          onSelect={() => onSelect(skill.name)}
          trailing={
            <Switch checked={!skill.archived} onChange={(on) => onToggle(skill.name, on)} />
          }
        />
      ))}
    </>
  );
}

export function ToolList({
  tools,
  query,
  chip,
  onChip,
  selected,
  onSelect,
}: {
  tools: readonly ToolRow[];
  query: string;
  chip?: string;
  onChip: (chip?: string) => void;
  selected?: string;
  onSelect: (name: string) => void;
}) {
  const s = t().skills;
  const counts = new Map<string, number>();
  for (const tool of tools) {
    if (tool.toolset !== undefined) counts.set(tool.toolset, (counts.get(tool.toolset) ?? 0) + 1);
  }
  const rows = tools.filter(
    (tool) =>
      matches(query, tool.name, tool.description) && (chip === undefined || tool.toolset === chip),
  );
  return (
    <>
      <Chips counts={counts} chip={chip} onChip={onChip} allLabel={s.chipAll} total={tools.length} />
      {rows.length === 0 && <p className="px-2.5 text-xs text-text-tertiary">{s.toolsEmpty}</p>}
      {rows.map((tool) => (
        <Row
          key={tool.name}
          name={tool.name}
          description={tool.toolset ?? tool.description}
          dimmed={!tool.enabled}
          active={selected === tool.name}
          onSelect={() => onSelect(tool.name)}
          trailing={
            // ponytail: display-only — no tool-toggle RPC exists; config
            // `tools.disabled` owns it. Make interactive when config.set lands.
            <span title={s.toolConfigManaged}>
              <Switch checked={tool.enabled} onChange={() => {}} disabled />
            </span>
          }
        />
      ))}
    </>
  );
}
