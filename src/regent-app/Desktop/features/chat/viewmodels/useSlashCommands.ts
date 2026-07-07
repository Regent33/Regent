// The `/`-completion command catalog — fetched once (module-level cache, the
// list is static for the process lifetime) via `commands.list` and reused by
// every Composer instance. `enabled` gates the first fetch so plain (non-Tauri)
// dev/prerender never issues the RPC.
import { useEffect, useState } from 'react';
import { deaconRequest } from '@/shared/infrastructure/rpc/client';

export interface SlashCommand {
  readonly name: string;
  readonly description: string;
}

let cache: SlashCommand[] | undefined;
let inFlight: Promise<void> | undefined;

export function useSlashCommands(enabled: boolean): readonly SlashCommand[] {
  const [commands, setCommands] = useState<readonly SlashCommand[]>(cache ?? []);

  useEffect(() => {
    if (!enabled || cache !== undefined) return;
    let alive = true;
    inFlight ??= deaconRequest<SlashCommand[]>('commands.list', {}).then((result) => {
      if (result.ok && Array.isArray(result.value)) cache = result.value;
    });
    void inFlight.then(() => {
      if (alive && cache !== undefined) setCommands(cache);
    });
    return () => {
      alive = false;
    };
  }, [enabled]);

  return commands;
}
