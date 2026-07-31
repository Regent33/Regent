// What actually goes ON a Butler diagram — the labels, not the transcript.
//
// This used to take the reply, strip its bullet markers, flatten it to one
// line, and cut the first eight SENTENCES at 72 characters each. A "diagram"
// built that way is the conversation in boxes: whole spoken sentences as
// timeline steps, conversational lead-ins ("Sure! Let me walk you through
// this.") as mind-map branches. The reported problem, in one line: everything
// the user said and everything Regent said bled onto the stage.
//
// Two things fix that. First, when the model enumerated its own points, THOSE
// are the relevant ones — the old code deliberately destroyed that structure
// before looking for it. Second, a node label is a phrase, not a sentence, so
// what survives is the leading clause, cut on a word boundary.

/** A node label is read at a glance from across a room. Past this it stops
 * being a label and starts being a paragraph someone has to read. */
const MAX_LABEL_CHARS = 48;

/** Ceiling on nodes. Beyond this a diagram is a list with extra steps. */
const MAX_POINTS = 8;

/** Conversational scaffolding a spoken answer opens and closes with. It is
 * addressed to the listener, never a thing on the diagram. */
const FILLER =
  /^(?:sure|of course|certainly|absolutely|ok(?:ay)?|alright|right|well|so|now|great question|good question|let me (?:explain|walk|break|show)|here(?:'s| is) (?:how|what|why|the)|i(?:'ll| will) (?:explain|walk|break)|hope (?:that|this) helps|does that (?:help|make sense)|in (?:short|summary)|to (?:summari[sz]e|recap)|anything else)\b/i;

/** The leading clause, cut on a word boundary. A label is a phrase. */
function toLabel(text: string): string {
  // Cut at the first clause boundary — a comma, dash or colon usually separates
  // the point from its elaboration, and the point is the part worth showing.
  const clause = text.split(/\s+[—–-]\s+|,\s+(?=(?:which|where|and|but|so|because)\b)|:\s+/)[0].trim();
  const trimmed = clause.replace(/[.!?;:,]+$/, '').trim();
  if (trimmed.length <= MAX_LABEL_CHARS) return trimmed;
  const cut = trimmed.slice(0, MAX_LABEL_CHARS);
  const lastSpace = cut.lastIndexOf(' ');
  return `${(lastSpace > 12 ? cut.slice(0, lastSpace) : cut).trim()}…`;
}

/** Lines the model itself enumerated — bullets or numbers. These are its own
 * statement of what the relevant points ARE, which beats any guess made by
 * re-reading its prose. */
function enumerated(body: string): string[] {
  return body
    .split(/\r?\n/)
    .filter((line) => /^\s*(?:[-*•]|\d+[.)])\s+\S/.test(line))
    .map((line) => line.replace(/^\s*(?:[-*•]|\d+[.)])\s+/, '').trim());
}

/** Sentences, for a reply with no list in it. */
function sentences(body: string): string[] {
  return body
    .replace(/\s+/g, ' ')
    .split(/(?<=[.!?])\s+/)
    .map((part) => part.trim());
}

/**
 * The relevant points from `reply`, as diagram labels. Empty when the reply
 * carries nothing worth drawing — which is what keeps a conversational turn
 * from getting a diagram at all, since the caller treats empty as "no visual".
 */
export function explanationPoints(reply: string): string[] {
  const body = reply
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/https?:\/\/\S+/g, ' ')
    .trim();
  if (body === '') return [];

  // A single bullet is a stray dash in prose, not an enumeration.
  const listed = enumerated(body);
  const candidates = listed.length >= 2 ? listed : sentences(body);

  return candidates
    .filter((part) => !FILLER.test(part))
    .map(toLabel)
    .filter((part) => part.length >= 3)
    .slice(0, MAX_POINTS);
}
