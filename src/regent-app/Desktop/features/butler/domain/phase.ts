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
}

export const initialButlerState: ButlerState = {
  phase: 'connecting',
  heard: '',
  reply: '',
  error: null,
  log: [],
};
