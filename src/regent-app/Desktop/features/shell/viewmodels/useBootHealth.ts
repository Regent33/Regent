'use client';
// Detects a dead deacon backend two ways: (1) `deacon.exited` — it was
// running, then its stdout pipe closed (deacon/rpc.rs). (2) it never came up
// at all — commands.rs returns "deacon is not running" for every request
// when spawn failed, so a bounded probe loop that never once succeeds is
// treated the same as exited. Once any probe succeeds, only (1) applies from
// then on — a later request timeout is a transient issue, not a dead process.
import { useEffect, useState } from 'react';
import { deaconRequest, isTauri } from '@/shared/infrastructure/rpc/client';
import { useDeaconExited, useLastDeaconError } from '@/shared/state/deaconBus';

const PROBE_INTERVAL_MS = 500;
const NEVER_STARTED_DEADLINE_MS = 20_000;

export interface BootHealth {
  readonly dead: boolean;
  readonly message?: string;
}

export function useBootHealth(): BootHealth {
  const exited = useDeaconExited();
  const exitedMessage = useLastDeaconError();
  const [neverStarted, setNeverStarted] = useState(false);
  const [lastMessage, setLastMessage] = useState<string>();

  useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    let everSucceeded = false;
    const deadline = Date.now() + NEVER_STARTED_DEADLINE_MS;

    const probe = async () => {
      while (alive && !everSucceeded && Date.now() < deadline) {
        const result = await deaconRequest('status.get', {});
        if (!alive) return;
        if (result.ok) {
          everSucceeded = true;
          return;
        }
        setLastMessage(result.error.message);
        await new Promise((r) => setTimeout(r, PROBE_INTERVAL_MS));
      }
      if (alive && !everSucceeded) setNeverStarted(true);
    };
    void probe();

    return () => {
      alive = false;
    };
  }, []);

  return { dead: exited || neverStarted, message: exited ? exitedMessage : lastMessage };
}
