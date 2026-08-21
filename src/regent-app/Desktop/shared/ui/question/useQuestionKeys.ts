// Keyboard owner for a question card: ↑↓ move, 1-9 jump, Space toggle, Enter
// submit, Esc skip — the same bindings the CLI card uses, so the two surfaces
// are muscle-memory compatible.
//
// One element holds focus (the card body) and names the highlighted row with
// aria-activedescendant, rather than a focusable button per row. That keeps a
// single keydown handler, and makes the Tab cycle below a two-line trap instead
// of a roving-tabindex dance.
import { useEffect, useRef } from 'react';
import type { KeyboardEvent, RefObject } from 'react';

/** Everything inside the card that can take focus, for the Tab cycle. */
const FOCUSABLE = 'button:not([disabled]), textarea:not([disabled]), [tabindex="0"]';

export interface QuestionKeys {
  readonly ref: RefObject<HTMLDivElement | null>;
  readonly onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
}

export interface QuestionKeysOptions {
  readonly rowCount: number;
  /** False while the free-text box owns the keyboard, or once answered. */
  readonly active: boolean;
  readonly onMove: (delta: number) => void;
  /** 0-based row index from a 1-9 keypress. */
  readonly onJump: (index: number) => void;
  readonly onToggle: () => void;
  readonly onSubmit: () => void;
  /** Undefined when the question is required — Esc then does nothing. */
  readonly onEscape?: () => void;
}

export function useQuestionKeys(options: QuestionKeysOptions): QuestionKeys {
  const { rowCount, active, onMove, onJump, onToggle, onSubmit, onEscape } = options;
  const ref = useRef<HTMLDivElement | null>(null);

  // The card takes the keyboard when it appears, and takes it back when the
  // free-text box closes: the turn is parked on this answer, so nothing else on
  // screen is waiting for a keystroke. preventScroll — the transcript already
  // auto-scrolled the card into view.
  useEffect(() => {
    if (active) ref.current?.focus({ preventScroll: true });
  }, [active]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // Trap Tab inside the card while a question is open. Esc, ✕ and Skip are
    // the ways out; Tab wandering off to the composer would leave a turn parked
    // on a card the keyboard can no longer reach.
    if (event.key === 'Tab' && ref.current) {
      const stops = [...ref.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (stops.length === 0) return;
      const at = stops.indexOf(document.activeElement as HTMLElement);
      const next = event.shiftKey ? at - 1 : at + 1;
      const wrapped = ((next % stops.length) + stops.length) % stops.length;
      event.preventDefault();
      stops[wrapped]?.focus();
      return;
    }
    // A focused control answers for itself — the free-text box needs its own
    // Enter/Space, and Skip/✕ need their own activation.
    if (!active || event.target !== event.currentTarget) return;

    if (event.key === 'Escape') {
      if (onEscape === undefined) return;
      event.preventDefault();
      onEscape();
      return;
    }
    if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
      event.preventDefault();
      onMove(event.key === 'ArrowUp' ? -1 : 1);
      return;
    }
    if (/^[1-9]$/.test(event.key)) {
      const index = Number(event.key) - 1;
      if (index >= rowCount) return;
      event.preventDefault();
      onJump(index);
      return;
    }
    if (event.key === ' ') {
      event.preventDefault();
      onToggle();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      onSubmit();
    }
  };

  return { ref, onKeyDown };
}
