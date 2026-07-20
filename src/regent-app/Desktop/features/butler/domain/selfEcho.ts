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
  if (said.length < RUN_TOKENS || reply.length < RUN_TOKENS) return false;
  // Any RUN_TOKENS-long window of the transcript appearing verbatim in the
  // reply is echo. Reply text streams AHEAD of the audio, so it is a superset
  // of what has actually been spoken — matching against it is safe.
  const joined = ` ${reply.join(' ')} `;
  for (let i = 0; i + RUN_TOKENS <= said.length; i++) {
    if (joined.includes(` ${said.slice(i, i + RUN_TOKENS).join(' ')} `)) return true;
  }
  return false;
}
