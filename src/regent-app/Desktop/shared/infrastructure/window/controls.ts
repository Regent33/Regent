// Native-window seam. Presentation never imports @tauri-apps/api directly —
// the Titlebar calls these; outside the desktop shell every call is a no-op
// and `available()` lets the UI hide the controls entirely.
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isTauri } from '@/shared/infrastructure/rpc/client';

export const windowControls = {
  available: (): boolean => isTauri(),
  minimize: (): void => {
    if (isTauri()) void getCurrentWindow().minimize();
  },
  toggleMaximize: (): void => {
    if (isTauri()) void getCurrentWindow().toggleMaximize();
  },
  close: (): void => {
    if (isTauri()) void getCurrentWindow().close();
  },
};
