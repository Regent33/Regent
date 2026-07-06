// Tool detail — tools.list already carries the full row, so this is a plain
// render with no extra fetch.
import { t } from '@/shared/i18n/t';
import type { ToolRow } from '@/features/skills/viewmodels/useToolsList';

export function ToolDetailPane({ tool }: { tool: ToolRow }) {
  const s = t().skills;
  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-text-primary">{tool.name}</h2>
      <p className="mt-1 text-xs text-text-tertiary">
        {tool.toolset ?? '—'}
        {!tool.enabled && ` · ${s.disabled}`}
      </p>
      {tool.description !== undefined && (
        <p className="mt-4 text-sm leading-relaxed text-text-primary">{tool.description}</p>
      )}
    </div>
  );
}
