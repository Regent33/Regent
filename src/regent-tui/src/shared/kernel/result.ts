// Result/Either — the project's error channel. Fallible operations return a
// Result instead of throwing; exceptions are never control flow (Section 5).

export interface Ok<T> {
  readonly ok: true;
  readonly value: T;
}

export interface Err<E> {
  readonly ok: false;
  readonly error: E;
}

export type Result<T, E = Failure> = Ok<T> | Err<E>;

export const ok = <T>(value: T): Ok<T> => ({ ok: true, value });
export const err = <E>(error: E): Err<E> => ({ ok: false, error });

export const isOk = <T, E>(r: Result<T, E>): r is Ok<T> => r.ok;
export const isErr = <T, E>(r: Result<T, E>): r is Err<E> => !r.ok;

/** Unwrap the value or fall back — never throws. */
export const unwrapOr = <T, E>(r: Result<T, E>, fallback: T): T => (r.ok ? r.value : fallback);

/** Base failure shape. Every typed error in the app narrows on `kind`. */
export interface Failure {
  readonly kind: string;
  readonly message: string;
  readonly cause?: unknown;
}

export const failure = (kind: string, message: string, cause?: unknown): Failure =>
  cause === undefined ? { kind, message } : { kind, message, cause };
