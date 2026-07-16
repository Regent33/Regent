'use client';
// Left pane: the real profile list (profile.list) — click a row to switch
// the active profile; "+" reveals an inline name field (slug: a-z 0-9 -)
// that creates one via profile.create.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Button } from '@/shared/ui/Button';
import { ListRow } from '@/shared/ui/ListRow';
import { PlusIcon, UserIcon } from '@/shared/ui/icons';
import type { ProfilesState } from '@/features/profiles/viewmodels/useProfiles';

export function ProfileList({
  state,
  skillCount,
}: {
  state: ProfilesState;
  skillCount?: number;
}) {
  const s = t().profiles;
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');

  const submit = async () => {
    const slug = name.trim();
    if (slug === '') return;
    if (await state.create(slug)) {
      setName('');
      setAdding(false);
      await state.switchTo(slug);
    }
  };

  return (
    <div className="flex h-full w-64 shrink-0 flex-col border-r border-stroke-tertiary p-2">
      <div className="flex items-center justify-between px-1 pb-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.08em] text-text-tertiary">{s.title}</span>
        <Button
          variant="ghost"
          size="iconSm"
          title={s.newProfile}
          aria-label={s.newProfile}
          onClick={() => setAdding((v) => !v)}
        >
          <PlusIcon />
        </Button>
      </div>

      {adding && (
        <form
          className="px-1 pb-2"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value.toLowerCase())}
            placeholder={s.newProfileHint}
            aria-label={s.newProfile}
            className="w-full rounded-md border border-stroke-tertiary bg-transparent px-2 py-1 text-xs text-text-primary placeholder:text-text-tertiary focus:outline-none focus:ring-1 focus:ring-accent"
          />
        </form>
      )}
      {state.error !== undefined && (
        <p className="px-1 pb-2 text-[10px] text-danger" role="alert">
          {state.error}
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {state.profiles.map((profile) => (
          <ListRow
            key={profile}
            icon={<UserIcon />}
            label={profile}
            description={
              profile === state.active && skillCount !== undefined
                ? `${skillCount} ${s.skillCount}`
                : undefined
            }
            active={profile === state.active}
            onClick={() => void state.switchTo(profile)}
            trailing={
              profile === state.active ? (
                <span className="rounded-full bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent">
                  {profile === 'default' ? s.defaultBadge : s.activeBadge}
                </span>
              ) : undefined
            }
          />
        ))}
      </div>
    </div>
  );
}
