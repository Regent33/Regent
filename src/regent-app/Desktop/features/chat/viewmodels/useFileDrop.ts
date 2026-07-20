'use client';
// Window-level file drag/drop for the composer.
//
// This only works because Tauri's OWN drag-drop handler is switched off
// (`dragDropEnabled: false` in tauri.conf.json). Left at its default of true,
// the Rust side swallows the OS drop and the webview is never sent a `drop`
// event at all — the reported "chat doesn't support drag and drop", with no
// error anywhere to explain it.
import { useEffect, useRef, useState } from 'react';

/** Text selections and dragged links also fire these events; only a real file
 *  drag advertises the "Files" type. */
const hasFiles = (transfer: DataTransfer | null) =>
  transfer !== null && Array.from(transfer.types).includes('Files');

/**
 * Reports whether files are being dragged over the window, and hands them to
 * `onFiles` when dropped anywhere in it. Listening on the window rather than on
 * a target around the composer matches how people actually drop a file into a
 * chat — over the conversation, not onto the narrow input bar.
 *
 * `enabled` gates only the callback and the overlay: the default-action
 * suppression below stays active regardless, because a file dropped on an
 * un-guarded webview NAVIGATES to it, discarding the running app.
 */
export function useFileDrop(onFiles: (files: FileList) => void, enabled: boolean): boolean {
  const [dragging, setDragging] = useState(false);
  const latest = useRef(onFiles);
  const live = useRef(enabled);

  useEffect(() => {
    latest.current = onFiles;
    live.current = enabled;
  }, [onFiles, enabled]);

  useEffect(() => {
    // dragenter/dragleave fire again for every child element the cursor
    // crosses, so a plain boolean flickers the overlay. Depth counting is the
    // standard remedy: only the leave balancing the outermost enter clears it.
    let depth = 0;
    const arm = (e: DragEvent) => {
      if (!hasFiles(e.dataTransfer)) return;
      e.preventDefault();
      depth += 1;
      if (live.current) setDragging(true);
    };
    const over = (e: DragEvent) => {
      if (!hasFiles(e.dataTransfer)) return;
      e.preventDefault(); // without this the drop is never delivered
      if (e.dataTransfer) e.dataTransfer.dropEffect = live.current ? 'copy' : 'none';
    };
    const leave = (e: DragEvent) => {
      if (!hasFiles(e.dataTransfer)) return;
      depth = Math.max(0, depth - 1);
      if (depth === 0) setDragging(false);
    };
    const drop = (e: DragEvent) => {
      if (!hasFiles(e.dataTransfer)) return;
      e.preventDefault();
      depth = 0;
      setDragging(false);
      const files = e.dataTransfer?.files;
      if (live.current && files && files.length > 0) latest.current(files);
    };
    window.addEventListener('dragenter', arm);
    window.addEventListener('dragover', over);
    window.addEventListener('dragleave', leave);
    window.addEventListener('drop', drop);
    return () => {
      window.removeEventListener('dragenter', arm);
      window.removeEventListener('dragover', over);
      window.removeEventListener('dragleave', leave);
      window.removeEventListener('drop', drop);
    };
  }, []);

  return dragging;
}
