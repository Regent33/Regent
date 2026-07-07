// Tracks whether a scrollable element is near its bottom edge, so the caller
// can (a) gate Transcript's own auto-scroll-on-new-content behind it and
// (b) show a floating scroll-to-bottom button once the user has scrolled away.
import { useEffect, useRef, useState, type RefObject } from 'react';

const NEAR_BOTTOM_PX = 200;

export interface AutoScroll<T extends HTMLElement> {
  readonly ref: RefObject<T | null>;
  readonly atBottom: boolean;
  readonly scrollToBottom: () => void;
}

export function useAutoScroll<T extends HTMLElement>(): AutoScroll<T> {
  const ref = useRef<T>(null);
  const [atBottom, setAtBottom] = useState(true);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => {
      const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
      setAtBottom(distance < NEAR_BOTTOM_PX);
    };
    onScroll();
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  const scrollToBottom = () => {
    const el = ref.current;
    if (!el) return;
    const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
    el.scrollTo({ top: el.scrollHeight, behavior: reduceMotion ? 'auto' : 'smooth' });
  };

  return { ref, atBottom, scrollToBottom };
}
