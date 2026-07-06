// Butler call phases — drives the particle core's state and the caption UI.
export type CallPhase = 'connecting' | 'listening' | 'thinking' | 'speaking';

export interface ButlerState {
  readonly phase: CallPhase;
  readonly heard: string;
  readonly reply: string;
  readonly error: string | null;
}

export const initialButlerState: ButlerState = {
  phase: 'connecting',
  heard: '',
  reply: '',
  error: null,
};
