// External-link seam: system browser via the opener plugin inside the shell,
// window.open in a plain browser. http(s) only — never navigate the app.
import { openUrl } from '@tauri-apps/plugin-opener';
import { isTauri } from '@/shared/infrastructure/rpc/client';

export function openExternal(href: string | undefined): void {
  if (href === undefined || !/^https?:\/\//.test(href)) return;
  if (isTauri()) void openUrl(href);
  else window.open(href, '_blank', 'noreferrer');
}
