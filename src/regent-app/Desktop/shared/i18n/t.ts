import { en } from './en';

/**
 * Trivial typed accessor for the active locale's strings. Only one locale
 * exists today; when a second lands, this is where the selection plumbing
 * goes (no i18n library needed for a single locale).
 */
export function t(): typeof en {
  return en;
}
