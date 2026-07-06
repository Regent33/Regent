'use client';
// Skills & Tools — master list (Skills from skills.list, Tools from
// tools.list) + a detail pane. Skills render their full body through
// SkillDetailPane (Markdown); tools render inline (their row already has
// everything, per tools.list's shape).
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { ListRow } from '@/shared/ui/ListRow';
import { useSkillsList } from '@/features/skills/viewmodels/useSkillsList';
import { useToolsList } from '@/features/skills/viewmodels/useToolsList';
import { SkillDetailPane } from '@/features/skills/presentation/SkillDetailPane';
import { ToolDetailPane } from '@/features/skills/presentation/ToolDetailPane';

type Selection = { kind: 'skill' | 'tool'; name: string } | undefined;

export function SkillsView() {
  const s = t().skills;
  const skills = useSkillsList();
  const tools = useToolsList();
  const [selected, setSelected] = useState<Selection>();

  const selectedTool = selected?.kind === 'tool' ? tools.tools.find((tl) => tl.name === selected.name) : undefined;

  return (
    <div className="flex h-full">
      <nav className="w-[240px] shrink-0 overflow-y-auto border-r border-stroke-tertiary p-2">
        <p className="px-2.5 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
          {s.skillsTitle}
        </p>
        {skills.loading && (
          <div className="flex justify-center py-2">
            <Loader />
          </div>
        )}
        {skills.error !== undefined && <ErrorState compact description={skills.error} />}
        {!skills.loading && skills.error === undefined && skills.skills.length === 0 && (
          <p className="px-2.5 text-xs text-text-tertiary">{s.skillsEmpty}</p>
        )}
        {skills.skills.map((skill) => (
          <ListRow
            key={skill.name}
            label={skill.name}
            description={skill.description}
            active={selected?.kind === 'skill' && selected.name === skill.name}
            onClick={() => setSelected({ kind: 'skill', name: skill.name })}
          />
        ))}

        <p className="px-2.5 pb-1 pt-4 text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">
          {s.toolsTitle}
        </p>
        {tools.loading && (
          <div className="flex justify-center py-2">
            <Loader />
          </div>
        )}
        {tools.error !== undefined && <ErrorState compact description={tools.error} />}
        {!tools.loading && tools.error === undefined && tools.tools.length === 0 && (
          <p className="px-2.5 text-xs text-text-tertiary">{s.toolsEmpty}</p>
        )}
        {tools.tools.map((tool) => (
          <ListRow
            key={tool.name}
            label={tool.name}
            description={tool.toolset}
            active={selected?.kind === 'tool' && selected.name === tool.name}
            onClick={() => setSelected({ kind: 'tool', name: tool.name })}
          />
        ))}
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
  );
}
