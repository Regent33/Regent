// Turn transport: WAV/text POST to the speech server and consumption of the
// streamed NDJSON reply (heard/reply/audio/error/noise lines).
import { SPEECH_URL } from '@/shared/infrastructure/voice/ensure';
import { voiceLanguageHint } from '@/shared/infrastructure/voice/protocol';
import type { CallSinks, PlaybackSink } from '@/features/butler/data/callTypes';
import { type Playing, fetchCallToken, playPcm, wavBytes } from '@/features/butler/data/speechClient';

export type TurnOutcome = 'normal' | 'noise';

/** One turn: WAV-encode the utterance, stream /call/turn, play each chunk. */
export async function runTurn(
  frames: Float32Array[],
  sampleRate: number,
  playback: PlaybackSink,
  sinks: CallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<TurnOutcome> {
  sinks.setPhase('thinking');
  let res: Response;
  try {
    const url = new URL(`${SPEECH_URL}/call/turn`);
    const language = voiceLanguageHint(
      typeof navigator === 'undefined' ? undefined : navigator.language,
    );
    if (language) url.searchParams.set('language', language);
    res = await fetch(url, {
      method: 'POST',
      body: wavBytes(frames, sampleRate),
      headers: { 'x-call-token': await fetchCallToken() },
      signal,
    });
  } catch (e) {
    if ((e as Error).name === 'AbortError') return 'normal'; // barged over
    sinks.setError('Speech server unreachable — reopen Butler Mode to restart it.');
    sinks.setPhase('listening');
    return 'normal';
  }
  return consumeTurnResponse(res, playback, sinks, signal, playing, onProgress);
}

/** Typed turn: same streamed response and voice path, without WAV/ASR. */
export async function runTextTurn(
  text: string,
  playback: PlaybackSink,
  sinks: CallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<TurnOutcome> {
  sinks.setPhase('thinking');
  let res: Response;
  try {
    res = await fetch(`${SPEECH_URL}/call/text`, {
      method: 'POST',
      body: text,
      headers: {
        'content-type': 'text/plain; charset=utf-8',
        'x-call-token': await fetchCallToken(),
      },
      signal,
    });
  } catch (e) {
    if ((e as Error).name === 'AbortError') return 'normal';
    sinks.setError('Speech server unreachable — reopen Butler Mode to restart it.');
    sinks.setPhase('listening');
    return 'normal';
  }
  return consumeTurnResponse(res, playback, sinks, signal, playing, onProgress);
}

async function consumeTurnResponse(
  res: Response,
  playback: PlaybackSink,
  sinks: CallSinks,
  signal: AbortSignal,
  playing: Playing,
  onProgress: () => void,
): Promise<TurnOutcome> {
  if (res.status === 401 || res.status === 403) {
    // Running server without our CORS/token grant (started by the CLI).
    sinks.setError(
      'The voice server is running without desktop access. Stop it (Stop-Process -Name regent-voice-server) and reopen Butler Mode.',
    );
    sinks.setPhase('listening');
    return 'normal';
  }
  if (!res.ok) {
    const failure = (await res.json().catch(() => ({}))) as { error?: string };
    sinks.setError(failure.error || `Speech server rejected the turn (${res.status}).`);
    sinks.setPhase('listening');
    return 'normal';
  }
  if (!res.body) {
    sinks.setPhase('listening');
    return 'normal';
  }
  sinks.setError(null);

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let acc = '';
  let spokeYet = false;
  let replySeen = false;
  let visualAwaited = false;
  let noiseRejected = false;
  // Keep reading while ordered audio plays. This is essential when a filler
  // arrives before the completed diagram spec: the later reply can still be
  // consumed and render/release the visual instead of deadlocking the stream.
  let audioQueue = Promise.resolve();
  try {
    for (;;) {
      if (signal.aborted) return 'normal';
      const { done, value } = await reader.read();
      if (done) break;
      acc += decoder.decode(value, { stream: true });
      let nl: number;
      while ((nl = acc.indexOf('\n')) >= 0) {
        const line = acc.slice(0, nl);
        acc = acc.slice(nl + 1);
        if (!line.trim()) continue;
        let msg: {
          heard?: string;
          reply?: string;
          filler?: string;
          audio?: string;
          error?: string;
          noise?: { reason?: string } | string;
        };
        try {
          msg = JSON.parse(line);
        } catch {
          continue;
        }
        onProgress(); // any valid line (incl. keepalives) proves the turn is alive
        // Empty ASR is an ambient/noise miss, not something the caller said.
        if (typeof msg.heard === 'string' && msg.heard.trim() !== '') sinks.setHeard(msg.heard);
        if (typeof msg.reply === 'string') {
          replySeen = true;
          sinks.setReply(msg.reply);
        }
        // Not a reply — `replySeen` stays false so the visual gate still treats
        // the audio that follows as the thinking filler.
        if (typeof msg.filler === 'string') sinks.setFiller(msg.filler);
        if (msg.error) sinks.setError(msg.error);
        if (msg.noise) noiseRejected = true;
        if (typeof msg.audio === 'string') {
          const audio = msg.audio;
          const first = !spokeYet;
          spokeYet = true;
          // The server always streams a `reply` line before a sentence's audio,
          // so audio arriving before ANY reply is the thinking FILLER. The
          // visual gate holds the first REAL sentence until the diagram is
          // visible — but it must never hold the filler: gating it turned the
          // bridge line into dead air (it sat on the gate until the first reply
          // or the 12s fuse) and then played it late, queued AHEAD of the
          // actual answer — the "slow tool turns feel extra delayed" bug.
          const awaitVisual = replySeen && !visualAwaited;
          if (awaitVisual) visualAwaited = true;
          audioQueue = audioQueue.then(async () => {
            if (signal.aborted) return;
            if (awaitVisual) {
              await sinks.waitForVisual();
              if (signal.aborted) return;
            }
            if (first) sinks.setPhase('speaking');
            await playPcm(playback.ctx, playback.out, audio, signal, playing);
          });
        }
      }
    }
  } catch {
    return 'normal'; // aborted reader read → swallow
  }
  if (noiseRejected) {
    if (!signal.aborted) sinks.setPhase('listening');
    return 'noise';
  }
  // Hardening, not a diagnosed fix: this is the only sink in this function not
  // guarded by `signal.aborted`, and the loop's own guard sits BEFORE the read
  // that can return `done`. So an abort landing during that last read (a barge
  // verification the client rules self-echo) falls through to here and
  // finalizes the visual of the turn it was interrupting — releasing a diagram
  // gate that the live answer still owns. Narrow, but free to close.
  if (!signal.aborted) await sinks.finalizeVisual();
  await audioQueue.catch(() => undefined);
  if (!signal.aborted) sinks.setPhase('listening');
  return 'normal';
}
