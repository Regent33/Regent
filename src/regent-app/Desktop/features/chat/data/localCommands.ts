// CLI-parity slash commands: EVERY known command executes as direct deacon
// RPCs and renders locally in the transcript — a slash command never enters
// the model pipeline (bug 2026-07-13: unknown ones leaked to the model and
// the model "ran" them). Terminal-only commands answer with guidance instead.
import { deaconRequest } from '@/shared/infrastructure/rpc/client';

export interface SlashInvocation {
  readonly name: string;
  readonly args: string;
}

/** `/name rest of args` → invocation; anything else → undefined. */
export function parseSlashCommand(text: string): SlashInvocation | undefined {
  const m = /^\/([a-z][a-z_-]*)(?:\s+([\s\S]*))?$/i.exec(text.trim());
  if (m === null) return undefined;
  return { name: m[1].toLowerCase(), args: (m[2] ?? '').trim() };
}

/** Shell-style tokenizer: whitespace splits, `"…"`/`'…'` group (quotes
 *  stripped) — create-style commands carry quoted, space-containing args. */
export function tokenize(line: string): string[] {
  const parts: string[] = [];
  let current = '';
  let quote: '"' | "'" | null = null;
  let any = false;
  for (const ch of line) {
    if (quote !== null) {
      if (ch === quote) quote = null;
      else current += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      any = true;
    } else if (/\s/.test(ch)) {
      if (current !== '' || any) parts.push(current);
      current = '';
      any = false;
    } else {
      current += ch;
    }
  }
  if (current !== '' || any) parts.push(current);
  return parts;
}

/** `--key value` / `--key=value` flags + positionals from tokens. */
export function parseArgs(tokens: readonly string[]): {
  positionals: string[];
  flags: Record<string, string>;
} {
  const positionals: string[] = [];
  const flags: Record<string, string> = {};
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.startsWith('--')) {
      const eq = t.indexOf('=');
      if (eq > 2) {
        flags[t.slice(2, eq)] = t.slice(eq + 1);
      } else {
        const next = tokens[i + 1];
        if (next !== undefined && !next.startsWith('--')) {
          flags[t.slice(2)] = next;
          i++;
        } else {
          flags[t.slice(2)] = 'true';
        }
      }
    } else {
      positionals.push(t);
    }
  }
  return { positionals, flags };
}

type Row = Record<string, unknown>;

const fence = (v: unknown): string => `\`\`\`json\n${JSON.stringify(v, null, 2)}\n\`\`\``;
const str = (v: unknown): string => (typeof v === 'string' ? v : '');
const rows = (v: unknown): Row[] =>
  Array.isArray(v) ? v.filter((x): x is Row => typeof x === 'object' && x !== null) : [];

async function call(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
  const r = await deaconRequest(method, params);
  if (!r.ok) throw new Error(r.error.message);
  return r.value;
}

const HANDLERS: Record<string, (args: string) => Promise<string>> = {
  status: async () => fence(await call('status.get')),
  version: async () => {
    const v = (await call('version')) as Row;
    return `Regent deacon v${str(v.version) || '?'}`;
  },
  sessions: async () => {
    const list = rows(await call('session.list', { limit: 15 }));
    if (list.length === 0) return 'No sessions yet.';
    const lines = list.map((s) => {
      const title = str(s.title) || str(s.id).slice(0, 8);
      return `- **${title}** — ${str(s.source) || 'chat'}, ${String(s.message_count ?? '?')} messages (\`${str(s.id)}\`)`;
    });
    return `**Sessions**\n\n${lines.join('\n')}`;
  },
  model: async (args) => {
    if (args === '') {
      const v = (await call('model.get')) as Row;
      return `Active model: **${str(v.model) || 'unknown'}**`;
    }
    const v = (await call('model.set', { model: args })) as Row;
    return `Model set to **${str(v.model) || args}**${str(v.note) !== '' ? ` — ${str(v.note)}` : ''}`;
  },
  agents: async () => {
    const list = rows(await call('agents.list'));
    if (list.length === 0) return 'No named agents yet. Ask me to create one, or use the CLI: `regent agents set <name>`.';
    const lines = list.map(
      (a) => `- **${str(a.name)}** — ${str(a.description) || 'no description'}${str(a.model) !== '' ? ` (${str(a.model)})` : ''}`,
    );
    return `**Agents**\n\n${lines.join('\n')}`;
  },
  cron: async () => {
    const list = rows(await call('cron.list'));
    if (list.length === 0) return 'No scheduled tasks.';
    const lines = list.map(
      (j) => `- ${j.enabled === true ? '🟢' : '⚪'} **${str(j.name) || str(j.id)}** — \`${str(j.schedule)}\``,
    );
    return `**Cron**\n\n${lines.join('\n')}`;
  },
  memory: async () => fence(await call('memory.list')),
  skills: async () => {
    const list = rows(await call('skills.list'));
    if (list.length === 0) return 'No skills installed.';
    return `**Skills**\n\n${list.map((s) => `- **${str(s.name)}** — ${str(s.description)}`).join('\n')}`;
  },
  tools: async () => {
    const list = rows(await call('tools.list'));
    return `**Tools** (${list.length})\n\n${list.map((t) => `- \`${str(t.name)}\``).join('\n')}`;
  },
  providers: async () => fence(await call('providers.list')),
  insights: async () => fence(await call('insights.get')),
  kanban: async () => fence(await call('kanban.list')),
  help: async () => {
    const list = rows(await call('commands.list'));
    return `**Commands**\n\n${list.map((c) => `- \`/${str(c.name)}\` — ${str(c.description)}`).join('\n')}`;
  },
};

// Subcommand-aware handlers (args pre-tokenized, quotes honored). Each maps a
// CLI shape onto the matching deacon RPC — no model round-trip.
const SUB_HANDLERS: Record<string, (pos: string[], flags: Record<string, string>) => Promise<string>> = {
  'model list': async () => fence(await call('model.list')),
  'agents show': async ([name]) => fence(await call('agents.show', { name })),
  'agents create': agentsSet,
  'agents add': agentsSet,
  'agents edit': agentsSet,
  'agents remove': async ([name]) => {
    await call('agents.remove', { name });
    return `Removed agent **${name ?? ''}**.`;
  },
  'kanban create': async (pos, flags) => {
    const v = (await call('kanban.create', {
      title: pos.join(' '),
      ...(flags.assignee !== undefined ? { assignee: flags.assignee } : {}),
    })) as Row;
    return `Created task \`${str(v.id)}\` — ${pos.join(' ')}`;
  },
  'kanban show': async ([id]) => fence(await call('kanban.show', { id })),
  'kanban assign': async ([id, worker]) => {
    await call('kanban.assign', { id, worker });
    return `Assigned \`${id ?? ''}\` → **${worker ?? ''}**.`;
  },
  'memory pending': async () => fence(await call('memory.pending')),
  'skills view': async ([name]) => {
    const v = (await call('skills.view', { name })) as Row;
    return `**${str(v.name) || name}**\n\n${str(v.body) || fence(v)}`;
  },
  'providers models': async ([name]) => fence(await call('providers.models', { name })),
  'providers test': async ([name]) => fence(await call('providers.test', { name })),
  'config show': async () => fence(await call('config.get')),
  'config set': async ([path, ...rest]) => {
    await call('config.set', { path, value: parseConfigValue(rest.join(' ')) });
    return `Set \`${path ?? ''}\` — applies to new sessions.`;
  },
  'mom run': async ([name, ...brief]) => {
    const v = (await call('mom.run', { name, brief: brief.join(' ') })) as Row;
    return str(v.synthesis) || fence(v);
  },
  'cron remove': async ([id]) => {
    await call('cron.remove', { id });
    return `Removed cron job \`${id ?? ''}\`.`;
  },
  'env list': async () => fence(await call('env.list')),
  'voice status': async () => fence(await call('voice.status')),
  'voice models': async () => fence(await call('voice.models')),
  'persona show': async () => fence(await call('persona.get')),
};

async function agentsSet(pos: string[], flags: Record<string, string>): Promise<string> {
  const name = pos[0];
  const v = (await call('agents.set', {
    name,
    ...(flags.description !== undefined ? { description: flags.description } : {}),
    ...(flags.prompt !== undefined ? { system_prompt: flags.prompt } : {}),
    ...(flags.model !== undefined ? { model: flags.model } : {}),
    ...(flags.tools !== undefined ? { tools: flags.tools } : {}),
  })) as Row;
  return `Saved agent **${str(v.name) || name || ''}**.`;
}

/** Kanban status verbs share one RPC. */
const KANBAN_STATUS: Record<string, string> = {
  start: 'in_progress',
  review: 'review',
  block: 'blocked',
  unblock: 'todo',
  complete: 'done',
};

/** Memory verbs sharing the `{id}` shape. */
const MEMORY_VERBS = new Set(['approve', 'reject', 'pin', 'unpin', 'forget']);

// These own a terminal/process (wizards, servers, process control, local
// config-file edits) — honest guidance beats silently handing them to the model.
const TERMINAL_ONLY = new Set([
  'setup', 'gateway', 'migrate', 'call', 'mcp', 'debug', 'logs',
  'doctor', 'security', 'auth', 'profile', 'keys', 'code', 'chat',
]);

/** Bare `2026-01-01`/numbers/bools parse typed; anything else stays a string. */
function parseConfigValue(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/** True when the command executes locally (never reaches the model). */
export function isLocalCommand(name: string): boolean {
  return (
    name in HANDLERS ||
    TERMINAL_ONLY.has(name) ||
    ['mom', 'config', 'env', 'voice', 'persona', 'soul', 'about'].includes(name)
  );
}

/** Run a local command; errors render as text so the exchange always ends. */
export async function runLocalCommand(cmd: SlashInvocation): Promise<string> {
  try {
    if (TERMINAL_ONLY.has(cmd.name)) {
      const full = `regent ${cmd.name} ${cmd.args}`.trim();
      return `\`/${cmd.name}\` needs a real terminal — run \`${full}\` there.`;
    }
    const tokens = tokenize(cmd.args);
    const sub = (tokens[0] ?? '').toLowerCase();
    const key = `${cmd.name} ${sub}`;
    const { positionals, flags } = parseArgs(tokens.slice(1));
    if (key in SUB_HANDLERS) return await SUB_HANDLERS[key](positionals, flags);
    if (cmd.name === 'kanban' && sub in KANBAN_STATUS) {
      await call('kanban.set_status', { id: positionals[0], status: KANBAN_STATUS[sub] });
      return `Task \`${positionals[0] ?? ''}\` → **${KANBAN_STATUS[sub]}**.`;
    }
    if (cmd.name === 'memory' && MEMORY_VERBS.has(sub)) {
      await call(`memory.${sub}`, { id: positionals[0] });
      return `Memory \`${positionals[0] ?? ''}\` — ${sub} done.`;
    }
    if (cmd.name === 'skills' && (sub === 'opt-out' || sub === 'opt-in')) {
      await call(`skills.${sub.replace('-', '_')}`, { name: positionals[0] });
      return `Skill **${positionals[0] ?? ''}** — ${sub} done.`;
    }
    if (cmd.name === 'mom') {
      // list is config-backed; create/remove edit config.yaml → terminal.
      if (sub === '' || sub === 'list') {
        const cfg = (await call('config.get')) as Row;
        const mom = (cfg.mom ?? {}) as Row;
        const names = Object.keys(mom);
        if (names.length === 0) return 'No MoM groups — create one: `regent mom create <name> --proposers a,b --aggregator c`.';
        return `**MoM groups**\n\n${names.map((n) => `- **${n}**`).join('\n')}\n\nRun one: \`/mom run <name> <brief>\``;
      }
      return 'MoM group setup edits config.yaml — run `regent mom create …` in a terminal; `/mom run <name> <brief>` works here.';
    }
    if (cmd.name === 'persona' || cmd.name === 'soul' || cmd.name === 'about') {
      return fence(await call('persona.get'));
    }
    if (cmd.name === 'env' && sub === 'set') {
      await call('env.set', { name: positionals[0], value: positionals[1] });
      return `Set \`${positionals[0] ?? ''}\`.`;
    }
    if (cmd.name === 'config' && sub === '') return fence(await call('config.get'));
    if (cmd.name in HANDLERS) return await HANDLERS[cmd.name](cmd.args);
    return `\`/${cmd.name} ${sub}\` isn't a recognized subcommand — \`/help\` lists commands.`;
  } catch (e) {
    return `\`/${cmd.name}\` failed: ${e instanceof Error ? e.message : String(e)}`;
  }
}
