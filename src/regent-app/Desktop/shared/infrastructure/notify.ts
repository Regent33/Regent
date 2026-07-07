'use client';
// Background-completion signal: mount `useTurnCompletionNotify` once (in
// Shell) and it fires a native OS notification plus the WebAudio chime
// whenever a turn completes while the window is NOT focused — alt-tabbed
// away, minimized, or the webview tab hidden. A focused turn stays silent;
// the transcript itself is the notification in that case. Outside the
// desktop shell (plain browser) the Tauri call no-ops via isTauri().
import { useEffect } from 'react';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';
import { subscribe } from '@/shared/state/deaconBus';
import { isTauri } from '@/shared/infrastructure/rpc/client';
import { playChime } from '@/shared/infrastructure/sound';
import { t } from '@/shared/i18n/t';

// Requested at most once per process lifetime — only when a notification is
// actually about to fire, never eagerly on mount, so the OS permission
// prompt never appears before the user has a reason to see it.
let permissionRequested = false;

async function ensurePermission(): Promise<boolean> {
  if (!isTauri()) return false;
  if (await isPermissionGranted()) return true;
  if (permissionRequested) return false;
  permissionRequested = true;
  const result = await requestPermission();
  return result === 'granted';
}

function isWindowUnfocused(): boolean {
  return document.hidden || !document.hasFocus();
}

/** Mounted once in the shell. Subscribes to every `turn.complete` (all
 * sessions) and, only when the window is unfocused at that moment, requests
 * notification permission lazily and fires the native notification + chime. */
export function useTurnCompletionNotify(): void {
  useEffect(() => {
    const unsubscribe = subscribe({ method: 'turn.complete' }, () => {
      if (!isWindowUnfocused()) return;
      playChime();
      void (async () => {
        if (!(await ensurePermission())) return;
        const s = t().shell.notify;
        sendNotification({ title: s.title, body: s.body });
      })();
    });
    return unsubscribe;
  }, []);
}
