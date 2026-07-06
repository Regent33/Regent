// Butler call phases — drives the voice mark's state and the caption UI.
export type CallPhase = 'connecting' | 'listening' | 'thinking' | 'speaking';

export interface CaptionEntry {
  readonly who: 'you' | 'regent';
  readonly text: string;
}

export interface ButlerState {
  readonly phase: CallPhase;
  readonly heard: string;
  readonly reply: string;
  readonly error: string | null;
  /** Finished exchanges, oldest first — the Conversation window's content. */
  readonly log: readonly CaptionEntry[];
  /** A spoken place ask ("where is X") — drives the full-bleed map backdrop. */
  readonly mapQuery: string | null;
  /** Links from the latest reply — auto-pops the Results window (extraction
   * lives in data/links.ts). */
  readonly links: readonly LinkCard[];
}

/** A presentable link Regent spoke about (site / video / picture). */
export interface LinkCard {
  readonly url: string;
  readonly title: string;
  readonly host: string;
  readonly youtubeId?: string;
  readonly isImage: boolean;
}

export const initialButlerState: ButlerState = {
  phase: 'connecting',
  heard: '',
  reply: '',
  error: null,
  log: [],
  mapQuery: null,
  links: [],
};
