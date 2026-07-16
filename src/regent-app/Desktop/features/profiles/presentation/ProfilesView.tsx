'use client';
// Profiles — master/detail: the real profile list on the left (profile.list /
// create / switch), the ACTIVE profile's SOUL.md + About editors on the
// right. The detail pane is keyed by the active profile so its editors
// re-fetch (and re-save) against the profile just switched to.
import { useProfileMeta } from '@/features/profiles/viewmodels/useProfileMeta';
import { useProfiles } from '@/features/profiles/viewmodels/useProfiles';
import { ProfileList } from '@/features/profiles/presentation/ProfileList';
import { ProfileDetail } from '@/features/profiles/presentation/ProfileDetail';

export function ProfilesView() {
  const { model, skillCount } = useProfileMeta();
  const profiles = useProfiles();
  return (
    <div className="flex h-full">
      <ProfileList state={profiles} skillCount={skillCount} />
      <ProfileDetail key={profiles.active} name={profiles.active} model={model} />
    </div>
  );
}
