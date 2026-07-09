'use client';
// Settings — left section rail (searchable) + detail pane (Hermes IA). Every
// section is real now: bound to a live deacon RPC where the schema has one,
// or an honest read-only/empty state where it doesn't (Safety, MCP — see
// each file's header for what was checked and found absent). The search
// filters sections by their label AND a static keyword list of the fields
// inside each — picking the only remaining match on Enter.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { ListRow } from '@/shared/ui/ListRow';
import { SearchField } from '@/shared/ui/SearchField';
import { ModelSection } from '@/features/settings/presentation/ModelSection';
import { VoiceSection } from '@/features/settings/presentation/VoiceSection';
import { MemorySection } from '@/features/settings/presentation/MemorySection';
import { AboutSection } from '@/features/settings/presentation/AboutSection';
import { AppearanceSection } from '@/features/settings/presentation/AppearanceSection';
import { ApiKeysSection } from '@/features/settings/presentation/ApiKeysSection';
import { AdvancedSection } from '@/features/settings/presentation/AdvancedSection';
import { ChatSection } from '@/features/settings/presentation/ChatSection';
import { WorkspaceSection } from '@/features/settings/presentation/WorkspaceSection';
import { SafetySection } from '@/features/settings/presentation/SafetySection';
import { GatewaySection } from '@/features/settings/presentation/GatewaySection';
import { McpSection } from '@/features/settings/presentation/McpSection';
import { ArchivedSection } from '@/features/settings/presentation/ArchivedSection';

type SectionId =
  | 'model'
  | 'voice'
  | 'memory'
  | 'about'
  | 'chat'
  | 'appearance'
  | 'workspace'
  | 'safety'
  | 'advanced'
  | 'gateway'
  | 'apiKeys'
  | 'mcp'
  | 'archived';

const REAL: readonly SectionId[] = [
  'model',
  'chat',
  'apiKeys',
  'gateway',
  'mcp',
  'voice',
  'memory',
  'workspace',
  'safety',
  'advanced',
  'appearance',
  'archived',
  'about',
];

// Field-level search terms per section (what a user would type to find a
// control that lives inside it). Static on purpose — no indexing framework.
const KEYWORDS: Partial<Record<SectionId, string>> = {
  model: 'model provider claude catalog switch current',
  voice: 'voice speech asr tts provider model whisper microphone speak listen',
  memory: 'memory context pending approve reject pin forget stored',
  about: 'about version build',
  appearance: 'appearance theme dark light system mode color display',
  apiKeys: 'api key credential secret token provider env anthropic openai',
  advanced: 'advanced cron tick interval scheduler config',
  chat: 'chat turn compaction context trigger protect limit tokens',
  workspace: 'workspace memory home directory embeddings semantic',
  safety: 'safety approval sandbox jail permission tool',
  gateway: 'gateway platform telegram slack discord whatsapp messenger webhook',
  mcp: 'mcp server model context protocol',
  archived: 'archived unarchive delete session',
};

export function SettingsView() {
  const s = t().settings;
  const [section, setSection] = useState<SectionId>('model');
  const [query, setQuery] = useState('');

  const matches = (id: SectionId): boolean => {
    const q = query.trim().toLowerCase();
    if (q === '') return true;
    return (s.sections[id].toLowerCase() + ' ' + (KEYWORDS[id] ?? '')).includes(q);
  };
  const hits = REAL.filter(matches);

  return (
    <div className="flex h-full">
      <nav className="w-[200px] shrink-0 overflow-y-auto border-r border-stroke-tertiary p-2">
        <SearchField
          label={s.searchLabel}
          placeholder={s.searchPlaceholder}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && hits.length >= 1) setSection(hits[0]);
          }}
        />
        {hits.map((id) => (
          <ListRow key={id} label={s.sections[id]} active={section === id} onClick={() => setSection(id)} />
        ))}
        {hits.length === 0 && <p className="px-2.5 pt-2 text-xs text-text-tertiary">{s.searchEmpty}</p>}
      </nav>
      <div className="min-w-0 flex-1 overflow-y-auto">
        {section === 'model' && <ModelSection />}
        {section === 'chat' && <ChatSection />}
        {section === 'apiKeys' && <ApiKeysSection />}
        {section === 'gateway' && <GatewaySection />}
        {section === 'mcp' && <McpSection />}
        {section === 'voice' && <VoiceSection />}
        {section === 'memory' && <MemorySection />}
        {section === 'workspace' && <WorkspaceSection />}
        {section === 'safety' && <SafetySection />}
        {section === 'advanced' && <AdvancedSection />}
        {section === 'appearance' && <AppearanceSection />}
        {section === 'archived' && <ArchivedSection />}
        {section === 'about' && <AboutSection />}
      </div>
    </div>
  );
}
