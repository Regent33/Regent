'use client';
// Left pane: the "New profile" affordance (disabled — no multi-profile RPCs
// exist yet, see usePersona) and the single "default" profile row. Layout
// leaves room for a real list once the backend grows profile.list/create.
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ListRow } from '@/shared/ui/ListRow';
import { PlusIcon, UserIcon } from '@/shared/ui/icons';

export function ProfileList({ skillCount }: { skillCount?: number }) {
  const s = t().profiles;
  return (
    <div className="flex h-full w-64 shrink-0 flex-col border-r border-stroke-tertiary p-2">
      <div className="flex items-center justify-between px-1 pb-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">{s.title}</span>
        <Button variant="ghost" size="iconSm" disabled title={s.newProfileSoon} aria-label={s.newProfile}>
          <PlusIcon />
        </Button>
      </div>
      <ListRow
        icon={<UserIcon />}
        label={s.defaultName}
        description={skillCount !== undefined ? `${skillCount} ${s.skillCount}` : undefined}
        active
        trailing={
          <span className="rounded-full bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent">
            {s.defaultBadge}
          </span>
        }
      />
    </div>
  );
}
