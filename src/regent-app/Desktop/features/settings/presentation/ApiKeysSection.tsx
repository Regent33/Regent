'use client';
// API Keys section — env.list rows grouped into collapsible panels (LLM /
// Messaging / Search / Speech) by each row's `group` field. Every row keeps its
// set/replace/remove actions. Values are never displayed; only the deacon's
// masked preview. env errors render verbatim. An older deacon that omits
// `group` reports every row as 'llm', so this degrades to one flat panel.
import { useState } from 'react';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { ChevronDownIcon } from '@/shared/ui/icons';
import { t } from '@/shared/i18n/t';
import { Section } from '@/features/settings/presentation/primitives';
import { ApiKeyRow } from '@/features/settings/presentation/ApiKeyRow';
import { useApiKeys, type EnvKey, type KeyGroup } from '@/features/settings/viewmodels/useApiKeys';

const GROUP_ORDER: readonly KeyGroup[] = ['llm', 'messaging', 'search', 'speech'];

export function ApiKeysSection() {
  const s = t().settings.apiKeys;
  const vm = useApiKeys();
  const heading: Record<KeyGroup, string> = {
    llm: s.llmHeading,
    messaging: s.messagingHeading,
    search: s.searchHeading,
    speech: s.speechHeading,
  };

  return (
    <Section title={s.title}>
      {vm.loading && <Loader />}
      {vm.error !== undefined && <ErrorState description={vm.error} />}
      {!vm.loading && vm.error === undefined && vm.keys.length === 0 && <EmptyState title={s.empty} />}
      {!vm.loading &&
        vm.error === undefined &&
        GROUP_ORDER.map((group) => {
          const rows = vm.keys.filter((k) => k.group === group);
          if (rows.length === 0) return undefined;
          return (
            <KeyGroupPanel
              key={group}
              title={heading[group]}
              rows={rows}
              defaultOpen={group === 'llm'}
              savingName={vm.savingName}
              onSave={vm.save}
              onRemove={vm.remove}
            />
          );
        })}
    </Section>
  );
}

const MAX_KEY_SLOTS = 8; // mirrors the deacon's MAX_KEY_SLOTS

/** Next free numbered slot for a base key, or undefined when full/numbered. */
function nextSlotName(base: string, all: readonly EnvKey[]): string | undefined {
  if (/_\d+$/.test(base)) return undefined; // numbered rows don't nest further
  for (let n = 2; n <= MAX_KEY_SLOTS; n++) {
    const name = `${base}_${n}`;
    if (!all.some((k) => k.name === name && k.set)) return name;
  }
  return undefined;
}

function KeyGroupPanel({
  title,
  rows,
  defaultOpen,
  savingName,
  onSave,
  onRemove,
}: {
  title: string;
  rows: readonly EnvKey[];
  defaultOpen: boolean;
  savingName?: string;
  onSave: (name: string, value: string) => void;
  onRemove: (name: string) => void;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="mt-4 first:mt-0">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 py-1 text-left text-sm font-semibold text-text-primary"
      >
        <ChevronDownIcon className={`size-3.5 text-text-tertiary transition-transform ${open ? '' : '-rotate-90'}`} />
        {title}
        <span className="text-xs font-normal text-text-tertiary">{rows.length}</span>
      </button>
      {open && (
        <div className="mt-1">
          {rows.map((entry) => (
            <ApiKeyRow
              key={entry.name}
              entry={entry}
              saving={savingName === entry.name}
              onSave={onSave}
              onRemove={onRemove}
              addSlotName={entry.set ? nextSlotName(entry.name, rows) : undefined}
            />
          ))}
        </div>
      )}
    </div>
  );
}
