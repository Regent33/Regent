'use client';
// The dynamic content-window cluster — owns the registry (useContentWindows)
// and renders it, plus the two side effects governing its lifecycle: open a
// window for each newly-promoted item, and clear the whole cluster once the
// call actually leaves the 'windows' stage (topic change). Split out of
// ButlerView to keep that file under the file-size cap.
import { useEffect, useRef } from 'react';
import { FloatingWindow } from '@/features/butler/presentation/FloatingWindow';
import { ContentWindow } from '@/features/butler/presentation/ContentWindow';
import { useContentWindows } from '@/features/butler/viewmodels/useContentWindows';
import type { ContentItem } from '@/features/butler/domain/content';
import type { PresentationMode } from '@/features/butler/domain/presentation';

export function ContentWindowsLayer({
  content,
  presentationKind,
  closeLabel,
  resizeLabel,
}: {
  content: readonly ContentItem[];
  presentationKind: PresentationMode['kind'];
  closeLabel: string;
  resizeLabel: string;
}) {
  const { windows, openContent, closeContent, closeAllContent, focusContent, moveContent, resizeContent } =
    useContentWindows();

  // Rich media (images/YouTube) Regent hands over pops its own content
  // window instead of a Results thumbnail (splitLinks in useButlerCall keeps
  // `content` non-empty only when a fresh batch was just promoted).
  useEffect(() => {
    for (const item of content) openContent(item);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- open on new content only
  }, [content]);

  // Topic-change cleanup: once the presentation stage actually LEAVES the
  // window cluster (not merely "isn't windows this render" — a turn can land
  // on 'map' the same tick a content window opens), close every content
  // window. The call ending unmounts this component, which discards the
  // registry state the same way.
  const wasWindows = useRef(false);
  useEffect(() => {
    if (wasWindows.current && presentationKind !== 'windows') closeAllContent();
    wasWindows.current = presentationKind === 'windows';
  }, [presentationKind, closeAllContent]);

  return (
    <>
      {windows.map((cw) => (
        <FloatingWindow
          key={cw.item.id}
          title={cw.item.title}
          closeLabel={closeLabel}
          resizeLabel={resizeLabel}
          resizable
          x={cw.x}
          y={cw.y}
          z={cw.z}
          width={cw.width}
          height={cw.height}
          onFocus={() => focusContent(cw.item.id)}
          onClose={() => closeContent(cw.item.id)}
          onMove={(x, y) => moveContent(cw.item.id, x, y)}
          onResize={(w, h) => resizeContent(cw.item.id, w, h)}
        >
          <ContentWindow item={cw.item} />
        </FloatingWindow>
      ))}
    </>
  );
}
