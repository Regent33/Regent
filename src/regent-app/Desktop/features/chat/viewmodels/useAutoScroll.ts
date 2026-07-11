// Tracks whether a scrollable element is near its bottom edge, so the caller
// can (a) gate Transcript's own auto-scroll-on-new-content behind it and
// (b) show a floating scroll-to-bottom button once the user has scrolled away.
//
// Content inside the container (markdown, Shiki-highlighted code blocks,
// lightbox images) can grow AFTER a scroll already landed — a smooth
// scrollIntoView/scrollTo call targets a fixed pixel and never re-checks, so
// that late growth used to leave a gap at the bottom (initial history load,
// and the arrow button both fell short). A ResizeObserver on the container's
// content watches for that growth and keeps nudging back to the true bottom
// as long as the user hasn't scrolled away — same gate as everything else
// here, so reading history mid-stream is never yanked down.
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
  // Mirrors `atBottom` for the scroll/resize listeners below (set up once,
  // so they'd otherwise close over a stale value).
  const atBottomRef = useRef(true);
  // True while a scrollToBottom animation is in flight. The animation's own
  // scroll events are still far from the bottom, so without this flag the
  // first frame cleared the optimistic pin — and any content growth
  // mid-flight then stranded the scroll short of the true bottom, making
  // "return to latest" need several clicks. Only a USER gesture (wheel /
  // touch) or arrival clears it.
  const animatingRef = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onScroll = () => {
      const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
      const next = distance < NEAR_BOTTOM_PX;
      if (animatingRef.current) {
        if (!next) return; // mid-flight: keep the optimistic pin
        animatingRef.current = false; // arrived
      }
      atBottomRef.current = next;
      setAtBottom(next);
    };
    onScroll();
    el.addEventListener('scroll', onScroll, { passive: true });
    // The user taking the wheel cancels a pinned flight — their gesture owns
    // the view again, and the next scroll event re-evaluates honestly.
    const cancelFlight = () => {
      animatingRef.current = false;
    };
    el.addEventListener('wheel', cancelFlight, { passive: true });
    el.addEventListener('touchmove', cancelFlight, { passive: true });
    // Fires when scrolling settles for ANY reason (arrival or a browser-side
    // cancellation) — re-evaluate honestly so the pin can't go stale.
    const onScrollEnd = () => {
      animatingRef.current = false;
      onScroll();
    };
    el.addEventListener('scrollend', onScrollEnd);

    // ResizeObserver reports the SCROLL CONTAINER's own (fixed) viewport
    // box, not its overflowing content — so it's attached to the content
    // element (the container's single child: Loader/Hero/Transcript's root)
    // instead. That child gets swapped wholesale as state resolves
    // (resuming → seeded → Transcript mounts fresh), so a MutationObserver
    // re-attaches the ResizeObserver to whichever child is current.
    let contentRO: ResizeObserver | undefined;
    const watchContent = () => {
      contentRO?.disconnect();
      const content = el.firstElementChild;
      if (!content) return;
      contentRO = new ResizeObserver(() => {
        if (!atBottomRef.current) return;
        // A direct jump, not scrollTo — this is a passive correction to a
        // position the view is already meant to be at, not a user gesture,
        // so it must never re-trigger (or fight) the bouncy smooth scroll.
        el.scrollTop = el.scrollHeight - el.clientHeight;
      });
      contentRO.observe(content);
    };
    watchContent();
    const childMO = new MutationObserver(watchContent);
    childMO.observe(el, { childList: true });

    return () => {
      el.removeEventListener('scroll', onScroll);
      el.removeEventListener('wheel', cancelFlight);
      el.removeEventListener('touchmove', cancelFlight);
      el.removeEventListener('scrollend', onScrollEnd);
      contentRO?.disconnect();
      childMO.disconnect();
    };
  }, []);

  const scrollToBottom = () => {
    const el = ref.current;
    if (!el) return;
    // Optimistic: we're about to be there, and this keeps the ResizeObserver
    // above pinning the view through any growth that lands mid-animation or
    // just after it — the observer guarantees the true bottom is reached.
    // The button hides immediately (perceived immediacy) instead of waiting
    // for the animation's scroll events to catch up.
    atBottomRef.current = true;
    setAtBottom(true);
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight;
    const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
    // Keep the bounce only for short hops, where it reads as motion. A
    // far-away "return to latest" jumps instantly — smooth-animating
    // thousands of pixels of heavy markdown is seconds of blur, which is
    // exactly the "doesn't return right away" complaint.
    const smooth = !reduceMotion && distance <= el.clientHeight * 2;
    animatingRef.current = smooth;
    el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' });
  };

  return { ref, atBottom, scrollToBottom };
}
