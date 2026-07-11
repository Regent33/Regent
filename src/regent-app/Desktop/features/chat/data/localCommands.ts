// CLI-parity slash commands: `/status`, `/sessions`, `/model` … execute as
// direct deacon RPCs and render locally in the transcript — they never enter
// the model pipeline (same behavior as the regent CLI's slash surface).
// Commands not listed here (skills, UI controls like /new) fall through to
// the normal submit path.
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

/** True when the command executes locally (never reaches the model). */
export function isLocalCommand(name: string): boolean {
  return name in HANDLERS;
}

/** Run a local command; errors render as text so the exchange always ends. */
export async function runLocalCommand(cmd: SlashInvocation): Promise<string> {
  try {
    return await HANDLERS[cmd.name](cmd.args);
  } catch (e) {
    return `\`/${cmd.name}\` failed: ${e instanceof Error ? e.message : String(e)}`;
  }
}
