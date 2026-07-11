// Barrel composing the domain-split i18n files under ./en/*. Consumers keep
// importing `t` from './t' (which reads `en` from here) — this file only
// assembles the pieces, it never holds copy itself.
import { core } from './en/core';
import { shell } from './en/shell';
import { messaging } from './en/messaging';
import { butler } from './en/butler';
import { chat } from './en/chat';
import { settings } from './en/settings';
import { skills } from './en/skills';
import { cron } from './en/cron';
import { artifacts } from './en/artifacts';
import { profiles } from './en/profiles';

export const en = {
  ...core,
  ...shell,
  ...messaging,
  ...butler,
  ...chat,
  ...settings,
  ...skills,
  ...cron,
  ...artifacts,
  ...profiles,
} as const;
