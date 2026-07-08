'use client';
// Settings — left section rail (searchable) + detail pane (Hermes IA).
// Model, Voice, Memory & Context, and About are wired to real deacon RPCs;
// the remaining Hermes section names render an honest "on the roadmap"
// EmptyState rather than fake controls. The search filters sections by their
// label AND a static keyword list of the fields inside each — picking the
// only remaining match on Enter.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { ListRow } from '@/shared/ui/ListRow';
import { EmptyState } from '@/shared/ui/EmptyState';
import { SearchField } from '@/shared/ui/SearchField';
import { ModelSection } from '@/features/settings/presentation/ModelSection';
import { VoiceSection } from '@/features/settings/presentation/VoiceSection';
import { MemorySection } from '@/features/settings/presentation/MemorySection';
import { AboutSection } from '@/features/settings/presentation/AboutSection';
import { AppearanceSection } from '@/features/settings/presentation/AppearanceSection';
import { ApiKeysSection } from '@/features/settings/presentation/ApiKeysSection';
import { AdvancedSection } from '@/features/settings/presentation/AdvancedSection';

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
  'apiKeys',
  'voice',
  'memory',
  'advanced',
  'appearance',
  'about',
];
const ROADMAP: readonly SectionId[] = ['chat', 'workspace', 'safety', 'gateway', 'mcp', 'archived'];

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
};

export function SettingsView() {
  const s = t().settings;
  const [section, setSection] = useState<SectionId>('model');
  const [query, setQuery] = useState('');
  const isRoadmap = ROADMAP.includes(section);

  const matches = (id: SectionId): boolean => {
    const q = query.trim().toLowerCase();
    if (q === '') return true;
    return (s.sections[id].toLowerCase() + ' ' + (KEYWORDS[id] ?? '')).includes(q);
  };
  const realHits = REAL.filter(matches);
  const roadmapHits = ROADMAP.filter(matches);

  return (
    <div className="flex h-full">
      <nav className="w-[200px] shrink-0 overflow-y-auto border-r border-stroke-tertiary p-2">
        <SearchField
          label={s.searchLabel}
          placeholder={s.searchPlaceholder}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            const only = [...realHits, ...roadmapHits];
            if (e.key === 'Enter' && only.length >= 1) setSection(only[0]);
          }}
        />
        {realHits.map((id) => (
          <ListRow key={id} label={s.sections[id]} active={section === id} onClick={() => setSection(id)} />
        ))}
        {realHits.length > 0 && roadmapHits.length > 0 && (
          <div className="my-2 border-t border-stroke-tertiary" />
        )}
        {roadmapHits.map((id) => (
          <ListRow key={id} label={s.sections[id]} active={section === id} onClick={() => setSection(id)} />
        ))}
        {realHits.length === 0 && roadmapHits.length === 0 && (
          <p className="px-2.5 pt-2 text-xs text-text-tertiary">{s.searchEmpty}</p>
        )}
      </nav>
      <div className="min-w-0 flex-1 overflow-y-auto">
        {section === 'model' && <ModelSection />}
        {section === 'apiKeys' && <ApiKeysSection />}
        {section === 'voice' && <VoiceSection />}
        {section === 'memory' && <MemorySection />}
        {section === 'advanced' && <AdvancedSection />}
        {section === 'appearance' && <AppearanceSection />}
        {section === 'about' && <AboutSection />}
        {isRoadmap && (
          <div className="flex h-full items-center justify-center">
            <EmptyState title={s.sections[section]} hint={t().ui.comingSoon} />
          </div>
        )}
      </div>
    </div>
  );
}
