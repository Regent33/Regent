// The last line of defence against Regent barging over himself.
//
// Every other guard is content-BLIND: energy, speech-shape, caller-loudness and
// playback-correlation all ask "does this look like someone talking?" — and
// Regent's own voice, re-entering the mic from the speakers, answers yes to all
// of them. The server's ASR is no better a judge: it is excellent at speech vs.
// noise and completely blind to WHOSE speech, so it transcribes his own reply
// perfectly and reports a confident `heard`.
//
// But the client knows something neither can: the exact words Regent is
// currently saying. If the "interruption" the server transcribed is a verbatim
// run of that reply, the mic heard the speakers, not the caller.
//
// Deliberately biased toward letting a barge THROUGH: only a contiguous run of
// several words settles it. A caller saying "wait" or "no, stop" shares no such
// run, while ASR of real echo reproduces the reply nearly verbatim.

/** A contiguous run this long, matched verbatim, is beyond coincidence. */
const RUN_TOKENS = 4;

/** Below this, a match proves nothing: "stop", "wait", "no" are exactly what a
 * real barge sounds like, and any of them may also appear in the reply. One
 * word always gets through. */
const MIN_TOKENS = 2;

/** Words only, lowercased — punctuation and spacing differ between the model's
 *  written reply and the ASR of its spoken form ("don't" vs "dont", "1990" vs
 *  "nineteen ninety"), so compare on the part that survives both. */
function tokenize(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s]/g, ' ')
    .split(/\s+/)
    .filter(Boolean);
}

/**
 * True when `heard` (what the server transcribed from a suspected interruption)
 * is really Regent's own `spoken` reply coming back through the microphone.
 */
export function isSelfEcho(heard: string, spoken: string): boolean {
  const said = tokenize(heard);
  const reply = tokenize(spoken);
  if (said.length < MIN_TOKENS || reply.length < MIN_TOKENS) return false;
  // The window is the transcript's own length when it is shorter than
  // RUN_TOKENS. It used to require RUN_TOKENS on BOTH sides, so a two- or
  // three-word fragment of Regent's own voice — which is most of what the mic
  // catches once the endpoint window widened — skipped this check entirely and
  // was promoted as a real interruption. A short fragment must match in FULL,
  // which is a stricter bar than the long case, not a looser one.
  const window = Math.min(RUN_TOKENS, said.length);
  // Any window-long run of the transcript appearing verbatim in the reply is
  // echo. Reply text streams AHEAD of the audio, so it is a superset of what
  // has actually been spoken — matching against it is safe.
  const joined = ` ${reply.join(' ')} `;
  for (let i = 0; i + window <= said.length; i++) {
    if (joined.includes(` ${said.slice(i, i + window).join(' ')} `)) return true;
  }
  return false;
}
