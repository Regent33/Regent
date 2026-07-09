'use client';
// Profiles — Hermes-parity master/detail: a profile list on the left, the
// selected profile's SOUL.md editor on the right. There's exactly one real
// profile today (no profile.list/create RPCs), so the list is a single
// "default" card and "New profile" is disabled — the layout still leaves
// room for a real multi-profile list once the backend grows one.
import { useProfileMeta } from '@/features/profiles/viewmodels/useProfileMeta';
import { ProfileList } from '@/features/profiles/presentation/ProfileList';
import { ProfileDetail } from '@/features/profiles/presentation/ProfileDetail';

export function ProfilesView() {
  const { model, skillCount } = useProfileMeta();
  return (
    <div className="flex h-full">
      <ProfileList skillCount={skillCount} />
      <ProfileDetail model={model} />
    </div>
  );
}
