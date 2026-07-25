// Update-check glue (ADR-041, Phase 0). Asks the deacon for its CACHED verdict
// over the existing `update.status` RPC with a short bounded timeout. Never
// throws and never fails the caller: an old deacon (method missing), a transport
// error, a timeout, or a malformed body all yield null so chat/doctor proceed
// exactly as before. The deacon never fetches on this path — it only reads its
// own background checker's cached snapshot.
import type { IRpcClient } from "@shared/kernel/contracts.ts";
import { parseUpdateStatus, type UpdateStatus, updateNotice } from "../domain/notice.ts";

/** Kept small: the deacon answers from cache, so this bounds a dead/old peer,
 *  not a network round-trip. */
const DEFAULT_TIMEOUT_MS = 1_500;

/** Fetch the deacon's cached update verdict, or null on any failure. */
export async function fetchUpdateStatus(
  client: IRpcClient,
  timeoutMs: number = DEFAULT_TIMEOUT_MS,
): Promise<UpdateStatus | null> {
  try {
    const res = await client.call<unknown>("update.status", {}, timeoutMs);
    return res.ok ? parseUpdateStatus(res.value) : null;
  } catch {
    // A misbehaving transport must never take down the host command.
    return null;
  }
}

/** The one-line notice to show, or null when there is nothing to say. */
export async function checkForUpdateNotice(
  client: IRpcClient,
  cliVersion: string,
  timeoutMs?: number,
): Promise<string | null> {
  return updateNotice(await fetchUpdateStatus(client, timeoutMs), cliVersion);
}
