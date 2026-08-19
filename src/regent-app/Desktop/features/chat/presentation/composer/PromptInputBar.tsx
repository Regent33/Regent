'use client';
import { useLayoutEffect, useRef, type KeyboardEvent, type ReactNode, type RefObject } from 'react';

const DEFAULT_MAX_ROWS = 7;

/** Collapse this far BEFORE the message actually reaches the controls.
 *
 *  Triggering on the collision itself is what made the transition impossible
 *  to get right: at that instant the message needs width it does not have, so
 *  the bar either grew a row and gave it back (a pop), or was held at its
 *  final height and cut the bottom off the text, and whichever way the field
 *  re-wrapped mid-transition while the controls were still moving through it.
 *
 *  With a margin the message still fits on one line when the controls leave,
 *  so nothing re-wraps, nothing pops, nothing is cut, and they fade out of
 *  space the text has not reached. About six characters' worth — enough to
 *  cover the ~200ms fade at any plausible typing speed. */
const TRIGGER_MARGIN_PX = 48;

export interface PromptInputBarProps {
  readonly value: string;
  readonly onChange: (value: string) => void;
  readonly onKeyDown?: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  readonly placeholder: string;
  readonly textareaRef: RefObject<HTMLTextAreaElement | null>;
  readonly ariaLabel?: string;
  readonly left?: ReactNode;
  readonly right?: ReactNode;
  readonly maxRows?: number;
  readonly disabled?: boolean;
  /** Fires when the message stops (or starts) fitting on one line, so the
   *  caller can step its wide controls out of the way. */
  readonly onExpandedChange?: (expanded: boolean) => void;
  /** How much width the field GAINS when the caller's collapsible controls
   *  step aside — 0 when they will not. The height is measured at the width
   *  the field ends at, so it never grows a row it is about to give back. */
  readonly collapsibleWidth?: number;
}

/** Grows the field to fit its text, up to `maxRows`, and reports whether that
 *  text still fits on ONE line.
 *
 *  Two DIFFERENT widths are involved, and using one for both is what made this
 *  misbehave twice over:
 *
 *  `narrowWidth` decides the wrap — the width the field has while the
 *  collapsible controls are showing. Judging both states there is what keeps
 *  the answer monotonic in the length of the message. Judge it at the live
 *  width instead and it feeds back on itself: hiding the controls widens the
 *  field, the text now fits on one line, the controls come back, it wraps
 *  again — every message between the two widths sits in that loop.
 *
 *  `gain` is how much wider the field ends up once those controls leave, and
 *  the HEIGHT is measured there. Measured at the live width instead, the box
 *  grew a row at the trigger and gave it straight back as the field widened —
 *  20px too tall for ~25ms, which is visible as a pop. */
function resizeTextarea(
  el: HTMLTextAreaElement,
  maxRows: number,
  narrowWidth: number,
  gain: number,
): boolean {
  const style = window.getComputedStyle(el);
  const parsedLineHeight = Number.parseFloat(style.lineHeight);
  const fontSize = Number.parseFloat(style.fontSize);
  const lineHeight = Number.isFinite(parsedLineHeight) ? parsedLineHeight : fontSize * 1.5;
  const paddingY = Number.parseFloat(style.paddingTop) + Number.parseFloat(style.paddingBottom);
  const borderY = Number.parseFloat(style.borderTopWidth) + Number.parseFloat(style.borderBottomWidth);
  const oneRow = lineHeight + paddingY + borderY;
  const maxHeight = lineHeight * maxRows + paddingY + borderY;

  el.style.height = 'auto';
  // What the text needs at `width`, without disturbing what is on screen.
  // `flex: none` is not decoration: this field is `flex-1`, so its computed
  // flex is `1 1 0%`, and on a flex item the basis BEATS `width`. Setting
  // width alone left it at its live width and measured nothing at all —
  // verified in the running app, width=200px still reported clientWidth 375,
  // and 200 only once flex was neutralised.
  const heightAt = (width: number): number => {
    if (width <= 0 || Math.abs(el.clientWidth - width) <= 1) return el.scrollHeight;
    const savedFlex = el.style.flex;
    el.style.flex = 'none';
    el.style.width = `${width}px`;
    const measured = el.scrollHeight;
    el.style.flex = savedFlex;
    el.style.width = '';
    return measured;
  };

  // Judged against a field a little narrower than the real one, so the
  // controls are already leaving by the time the message would have hit them.
  const wrapped = heightAt(Math.max(narrowWidth - TRIGGER_MARGIN_PX, 0)) > oneRow + 1;
  // Only a wrapped message makes the controls leave, so only then does the
  // field gain their width.
  const needed = heightAt(narrowWidth > 0 && wrapped ? narrowWidth + gain : narrowWidth);
  el.style.height = `${Math.min(needed, maxHeight)}px`;
  el.style.overflowY = needed > maxHeight ? 'auto' : 'hidden';
  return wrapped;
}

export function PromptInputBar({
  value,
  onChange,
  onKeyDown,
  placeholder,
  textareaRef,
  ariaLabel,
  left,
  right,
  maxRows = DEFAULT_MAX_ROWS,
  disabled = false,
  onExpandedChange,
  collapsibleWidth,
}: PromptInputBarProps) {
  const expandedRef = useRef(false);
  const narrowWidthRef = useRef(0);

  const measure = (el: HTMLTextAreaElement) => {
    // ALWAYS judged at the narrow width, collapsed or not. Measuring the
    // collapsed state at its live width sounds equivalent — it is the narrow
    // width, after all — but not while the controls are still sliding: the row
    // sweeps between the two widths for 200ms, so a keystroke landing in that
    // window gets judged at an in-between width and the answer changes with
    // the frame it lands on.
    const wrapped = resizeTextarea(el, maxRows, narrowWidthRef.current, collapsibleWidth ?? 0);
    if (!expandedRef.current) {
      // Only ever keep the SMALLER sample. For the 200ms after a collapse the
      // controls are still sliding back in, so clientWidth reads too wide, and
      // one too-wide sample would go on to judge a wrapped message as fitting.
      // Erring narrow costs an early collapse; erring wide costs the flicker.
      narrowWidthRef.current =
        narrowWidthRef.current === 0
          ? el.clientWidth
          : Math.min(narrowWidthRef.current, el.clientWidth);
    }
    if (wrapped !== expandedRef.current) {
      expandedRef.current = wrapped;
      onExpandedChange?.(wrapped);
    }
  };

  const measureRef = useRef(measure);

  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (el === null) return;
    measureRef.current = measure;
    measure(el);
  });

  useLayoutEffect(() => {
    const el = textareaRef.current;
    if (el === null) return;
    // The controls take 200ms to slide away, and the field is re-laid-out on
    // every frame of it — but nothing re-measured until the next keystroke, so
    // the box kept the height it had BEFORE the controls moved: a two-row box
    // holding one line of text, with the paperclip stranded at its bottom.
    // Height changes fire this observer too, hence the width guard, or setting
    // the height would call it straight back.
    let lastWidth = el.clientWidth;
    const observer = new ResizeObserver(() => {
      if (el.clientWidth === lastWidth) return;
      lastWidth = el.clientWidth;
      measureRef.current(el);
    });
    observer.observe(el);
    // A window resize is the one width change that invalidates the learned
    // narrow width rather than just needing a re-measure.
    const onResize = () => {
      narrowWidthRef.current = 0;
      measureRef.current(el);
    };
    window.addEventListener('resize', onResize);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', onResize);
    };
  }, [textareaRef]);

  return (
    <div
      // ONE row: the message sits between the paperclip and Send, not stacked
      // above them. A one-row bar used to leave barely half the width to type
      // in, because a long model name — "nemotron-3-ultra-…" — held its space
      // no matter how much was being written. The answer is for those controls
      // to leave once the message needs the room (see `onExpandedChange`), not
      // for the text to move to a line of its own.
      //
      // `items-end` keeps the buttons beside the LAST line as the field grows.
      // `min-w-0` lets the row absorb the squeeze when the panel is dragged
      // narrow. Deliberately NOT `overflow-hidden` — that stopped the spill
      // but clipped SEND off the end.
      className="flex min-w-0 items-end gap-1.5 rounded-2xl bg-bg px-2.5 py-2"
      style={{ boxShadow: 'var(--shadow-prompt)' }}
    >
      {left}
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        rows={1}
        aria-label={ariaLabel ?? placeholder}
        disabled={disabled}
        // py-2 makes one row exactly as tall as the 36px buttons beside it, so
        // the text is centred against them on one line and sits level with the
        // last line on many.
        className="min-w-0 flex-1 resize-none overflow-y-hidden bg-transparent px-1.5 py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary disabled:cursor-not-allowed disabled:opacity-60"
      />
      {right}
    </div>
  );
}
