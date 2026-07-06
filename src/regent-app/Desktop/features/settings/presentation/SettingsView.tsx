'use client';
// Settings — left section rail + detail pane (Hermes IA). Model, Voice,
// Memory & Context, and About are wired to real deacon RPCs; the remaining
// Hermes section names render an honest "on the roadmap" EmptyState rather
// than fake controls.
import { useState } from 'react';
import { t } from '@/shared/i18n/t';
import { ListRow } from '@/shared/ui/ListRow';
import { EmptyState } from '@/shared/ui/EmptyState';
import { ModelSection } from '@/features/settings/presentation/ModelSection';
import { VoiceSection } from '@/features/settings/presentation/VoiceSection';
import { MemorySection } from '@/features/settings/presentation/MemorySection';
import { AboutSection } from '@/features/settings/presentation/AboutSection';

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

const REAL: readonly SectionId[] = ['model', 'voice', 'memory', 'about'];
const ROADMAP: readonly SectionId[] = [
  'chat',
  'appearance',
  'workspace',
  'safety',
  'advanced',
  'gateway',
  'apiKeys',
  'mcp',
  'archived',
];

export function SettingsView() {
  const s = t().settings;
  const [section, setSection] = useState<SectionId>('model');
  const isRoadmap = ROADMAP.includes(section);

  return (
    <div className="flex h-full">
      <nav className="w-[200px] shrink-0 overflow-y-auto border-r border-stroke-tertiary p-2">
        {REAL.map((id) => (
          <ListRow key={id} label={s.sections[id]} active={section === id} onClick={() => setSection(id)} />
        ))}
        <div className="my-2 border-t border-stroke-tertiary" />
        {ROADMAP.map((id) => (
          <ListRow key={id} label={s.sections[id]} active={section === id} onClick={() => setSection(id)} />
        ))}
      </nav>
      <div className="min-w-0 flex-1 overflow-y-auto">
        {section === 'model' && <ModelSection />}
        {section === 'voice' && <VoiceSection />}
        {section === 'memory' && <MemorySection />}
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
