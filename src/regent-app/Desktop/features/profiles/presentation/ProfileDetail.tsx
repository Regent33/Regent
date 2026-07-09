'use client';
// Right pane: the default profile's identity (name + Default badge), its
// active model, and the SOUL.md editor. `model` is passed down from
// ProfilesView's single useProfileMeta call (model.get) — a plain read, no
// per-profile scoping exists in the backend.
import { t } from '@/shared/i18n/t';
import { SoulEditor } from '@/features/profiles/presentation/SoulEditor';

export function ProfileDetail({ model }: { model?: string }) {
  const s = t().profiles;

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <h1 className="text-lg font-semibold text-text-primary">{s.defaultName}</h1>
        <span className="rounded-full bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent">
          {s.defaultBadge}
        </span>
      </header>

      <p className="-mt-2 text-xs text-text-tertiary">
        {s.modelLabel}: <span className="font-mono text-text-secondary">{model ?? s.modelUnset}</span>
      </p>

      <SoulEditor />
    </div>
  );
}
