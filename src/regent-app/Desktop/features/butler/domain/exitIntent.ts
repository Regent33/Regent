// "Exit Butler mode" spoken aloud should actually leave Butler mode — reaching
// for the mouse to close a hands-free surface defeats the point of it.
//
// Deliberately narrow. This closes the whole surface, so a false positive
// destroys a conversation mid-flight; that is far worse than having to say it
// again. It therefore matches only an explicit command whose OBJECT is Butler
// itself (or a bare, unambiguous "close/exit" with nothing else said) — never
// "how do I exit vim", never "close the map", never a sentence that merely
// mentions leaving.

/** Verbs that end something, as a spoken command. */
const LEAVE = '(?:exit|close|quit|leave|end|stop|dismiss|shut down|turn off|get out of)';
/** How the caller refers to this surface. */
const BUTLER = "(?:butler(?: mode)?|this mode|the call|this call)";
/** Optional lead-in: an address, and/or a polite framing. */
const POLITE =
  "(?:(?:hey|ok|okay|yo)?\\s*regent[,\\s]+)?(?:please\\s+|can you\\s+|could you\\s+|would you\\s+|let'?s\\s+|i want to\\s+|i'?d like to\\s+)?";
/** Trailing filler that carries no new object — "exit NOW", "close please". */
const TAIL = '(?:\\s+(?:now|please|already|immediately|right now|right away))*';

// "exit butler mode", "close butler", "hey Regent, end the call", "let's exit
// butler mode now" — an optional lead-in and trailing filler are tolerated,
// but the object must be Butler.
const EXIT_BUTLER = new RegExp(
  `^${POLITE}${LEAVE}\\s+(?:out of\\s+)?(?:the\\s+|this\\s+)?${BUTLER}\\b`,
  'i',
);

// A bare command with no object — "exit.", "please exit now", "close butler
// mode please". Anchored at BOTH ends, and the only thing allowed after the
// verb is empty filler, so any real object disqualifies it: "stop, that's
// wrong" and "close the window on the left" never match here.
const BARE_EXIT = new RegExp(`^${POLITE}${LEAVE}(?:\\s+butler(?:\\s+mode)?)?${TAIL}[.!]?$`, 'i');

/**
 * True when the caller asked to LEAVE Butler mode. Punctuation and casing vary
 * with ASR, so both are normalized away before matching.
 */
export function isExitButlerCommand(heard: string): boolean {
  const said = heard
    .trim()
    .replace(/[.!?,]+$/, '')
    .replace(/\s+/g, ' ');
  if (said === '') return false;
  return EXIT_BUTLER.test(said) || BARE_EXIT.test(said);
}
