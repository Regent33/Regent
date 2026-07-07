'use client';
// Mounts the active route-replacing overlay (Settings, Skills) above the shell
// content, which stays mounted behind it. The command palette keeps its own
// chrome (CommandPalette); this host only drives the full-surface overlays.
import { t } from '@/shared/i18n/t';
import { Overlay } from '@/shared/ui/Overlay';
import { close, useCurrentOverlay } from '@/shared/state/overlays';
import { SettingsView } from '@/features/settings/presentation/SettingsView';
import { SkillsView } from '@/features/skills/presentation/SkillsView';

export function OverlayHost() {
  const current = useCurrentOverlay();

  if (current === 'settings') {
    return (
      <Overlay label={t().pages.settings} closeLabel={t().shell.titlebar.close} onClose={close}>
        <SettingsView />
      </Overlay>
    );
  }
  if (current === 'skills') {
    return (
      <Overlay label={t().pages.skills} closeLabel={t().shell.titlebar.close} onClose={close}>
        <SkillsView />
      </Overlay>
    );
  }
  return null;
}
