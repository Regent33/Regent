// External-link seam: system browser via the opener plugin inside the shell,
// window.open in a plain browser. http(s) only — never navigate the app.
import { openUrl } from '@tauri-apps/plugin-opener';
import { isTauri } from '@/shared/infrastructure/rpc/client';

export function openExternal(href: string | undefined): void {
  if (href === undefined || !/^https?:\/\//.test(href)) return;
  if (isTauri()) void openUrl(href);
  else window.open(href, '_blank', 'noreferrer');
}

/** Jump straight to Windows' mic privacy page — the OS permission popup
 * cannot be re-summoned once mic access is blocked, so this is the closest
 * one-click path. Scoped in capabilities/default.json; no-op off the shell. */
export function openMicPrivacySettings(): void {
  if (isTauri()) void openUrl('ms-settings:privacy-microphone');
}
