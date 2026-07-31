// Pure mapping helpers for chat events and stored history rows — parsing tool
// arg/result summaries into transcript disclosures, and staging attachments.
import { deaconRequest } from '@/shared/infrastructure/rpc/client';
import { splitPromptDecorations } from '@/shared/kernel/promptDecorations';
import {
  type ToolCodeDetail,
  type TranscriptItem,
} from '@/shared/kernel/transcript';

/** Cursor-style disclosure from a tool's args: the path being touched, or the
 * command being run. Summaries are truncated at 500 chars server-side, so the
 * JSON may not parse for huge inputs — undefined then, never a throw. */
export function detailFromArgs(name: string, argsSummary: unknown): string | undefined {
  if (typeof argsSummary !== 'string') return undefined;
  try {
    const a = JSON.parse(argsSummary) as Record<string, unknown>;
    if (name === 'terminal' && typeof a.command === 'string') return `$ ${a.command}`;
    if (typeof a.path === 'string') return a.path;
  } catch {
    return undefined;
  }
  return undefined;
}

export function codeFromArgs(value: unknown): ToolCodeDetail | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  const raw = value as Record<string, unknown>;
  if (raw.kind !== 'replace' && raw.kind !== 'write' && raw.kind !== 'patch') return undefined;
  const optional = (key: string): string | undefined =>
    typeof raw[key] === 'string' ? (raw[key] as string) : undefined;
  return {
    kind: raw.kind,
    path: optional('path'),
    before: optional('before'),
    after: optional('after'),
    patch: optional('patch'),
  };
}

/** Result-side disclosure: resolved path, line stats, a patch's file list —
 * or, on failure, the actual error text (clipped): a red icon alone tells the
 * user nothing about WHAT failed. */
export function detailFromResult(
  resultSummary: unknown,
): { detail?: string; adds?: number; dels?: number } {
  if (typeof resultSummary !== 'string') return {};
  try {
    const r = JSON.parse(resultSummary) as Record<string, unknown>;
    if (typeof r.error === 'string' && r.error !== '') {
      return { detail: clip(r.error, 160) };
    }
    const detail =
      typeof r.path === 'string'
        ? r.path
        : Array.isArray(r.applied)
          ? r.applied.filter((p): p is string => typeof p === 'string').join(' · ')
          : undefined;
    const adds = typeof r.lines_added === 'number' ? r.lines_added : undefined;
    const dels = typeof r.lines_removed === 'number' ? r.lines_removed : undefined;
    return { detail, adds, dels };
  } catch {
    return {};
  }
}

export const clip = (text: string, max: number): string =>
  text.length > max ? `${text.slice(0, max)}…` : text;

/** Base64 a File for `attachment.put` (deacon caps decoded size at 20 MB). */
export async function fileToBase64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

/**
 * Stage attachments BEFORE the turn: each returns a path under
 * $REGENT_HOME/attachments that prompt.submit references. Returns the staged
 * paths, or the upload error to report (never a silent send); `null` paths
 * mean the caller went away (`isAlive` false) — abort with no dispatch.
 */
export async function stageAttachments(
  sessionId: string,
  attachments: readonly File[],
  isAlive: () => boolean,
): Promise<{ paths: string[] } | { error: string } | null> {
  const paths: string[] = [];
  for (const file of attachments) {
    const put = await deaconRequest<{ path?: string }>('attachment.put', {
      session_id: sessionId,
      name: file.name,
      mime: file.type,
      data_base64: await fileToBase64(file),
    });
    if (!isAlive()) return null;
    if (!put.ok) return { error: put.error.message };
    if (typeof put.value?.path === 'string') paths.push(put.value.path);
  }
  return { paths };
}

export interface HistoryRow {
  readonly role?: string;
  readonly text?: string;
  readonly reasoning?: string | null;
  readonly tool_calls?: readonly string[];
  readonly tool_call_details?: readonly {
    readonly name?: string;
    readonly args_detail?: unknown;
  }[];
}

/** One stored row → transcript items (thinking → text → tool rows). */
/** Opening of the backend's stopped-turn placeholders (regent-kernel's
 * `NO_REPLY` and the interrupt variant) — matched on the shared prefix so a
 * new flavour of the note doesn't need a second constant here. */
const NO_REPLY_PREFIX = '(no reply — ';

export function rowToItems(m: HistoryRow): TranscriptItem[] {
  const items: TranscriptItem[] = [];
  if (m.role !== 'user' && m.role !== 'assistant') return items;
  if (typeof m.reasoning === 'string' && m.reasoning !== '') {
    items.push({ kind: 'thinking', text: m.reasoning });
  }
  // Tool calls run BEFORE the text of the same stored row lands — keep
  // chronological order when re-rendering.
  const detailed = m.tool_call_details ?? [];
  if (detailed.length > 0) {
    for (const call of detailed) {
      if (typeof call.name === 'string') {
        const code = codeFromArgs(call.args_detail);
        items.push({ kind: 'tool', name: call.name, done: true, code, detail: code?.path });
      }
    }
  } else {
    for (const name of m.tool_calls ?? []) {
      if (typeof name === 'string') items.push({ kind: 'tool', name, done: true });
    }
  }
  if (typeof m.text === 'string' && m.text !== '') {
    if (m.role === 'user') {
      // The stored text carries the deacon's own decorations (attachment refs,
      // the editor-context note) — lift them into chips so the bubble shows
      // what the user typed, not the plumbing that rode with it.
      const { text, attachments, context } = splitPromptDecorations(m.text);
      items.push({
        kind: 'user',
        text,
        ...(attachments.length > 0 ? { attachments } : {}),
        ...(context === undefined ? {} : { context }),
      });
    } else if (m.text.startsWith(NO_REPLY_PREFIX)) {
      // A stopped turn stores a short assistant note in place of the answer it
      // never gave, so the QUESTION stays in the model's context (barging in
      // used to delete it). It is bookkeeping, not something Regent said —
      // replay it as the same quiet line the live turn ended on.
      items.push({ kind: 'notice', text: m.text, tone: 'ok' });
    } else {
      items.push({ kind: 'assistant', text: m.text, streaming: false });
    }
  }
  return items;
}
