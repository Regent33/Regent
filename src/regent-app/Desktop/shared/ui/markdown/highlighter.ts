// Lazy singleton Shiki highlighter — the fine-grained core bundle + the pure
// JS regex engine (no WASM/Oniguruma fetch), so highlighting works inside the
// static export / Tauri webview with zero network calls (CSP blocks external
// hosts, and the WASM binary would otherwise need one). Only a small language
// subset ships; anything else falls back to a plain, unhighlighted block —
// callers check `isSupportedLanguage` first.
import { createHighlighterCore, type HighlighterCore } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';

// Dual themes: shiki inlines the light color plus a `--shiki-dark` var per
// token; globals.css flips tokens to the var under the app's dark selectors.
export const SHIKI_THEMES = {
  light: 'github-light-default',
  dark: 'github-dark-default',
} as const;

// Language id → dynamic import of its Shiki grammar. Covers the languages
// that show up in agent replies day to day; extend here as gaps appear.
const LANG_LOADERS: Record<string, () => Promise<unknown>> = {
  javascript: () => import('shiki/langs/javascript.mjs'),
  jsx: () => import('shiki/langs/jsx.mjs'),
  typescript: () => import('shiki/langs/typescript.mjs'),
  tsx: () => import('shiki/langs/tsx.mjs'),
  json: () => import('shiki/langs/json.mjs'),
  python: () => import('shiki/langs/python.mjs'),
  rust: () => import('shiki/langs/rust.mjs'),
  bash: () => import('shiki/langs/bash.mjs'),
  css: () => import('shiki/langs/css.mjs'),
  html: () => import('shiki/langs/html.mjs'),
  yaml: () => import('shiki/langs/yaml.mjs'),
  sql: () => import('shiki/langs/sql.mjs'),
  markdown: () => import('shiki/langs/markdown.mjs'),
  toml: () => import('shiki/langs/toml.mjs'),
  diff: () => import('shiki/langs/diff.mjs'),
};

// Fence-tag aliases that collapse onto one of the loaders above.
const ALIASES: Record<string, string> = {
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  ts: 'typescript',
  py: 'python',
  rs: 'rust',
  sh: 'bash',
  shell: 'bash',
  zsh: 'bash',
  yml: 'yaml',
  md: 'markdown',
};

/** Normalize a fence-info-string language tag to a Shiki loader key. */
export function resolveLanguage(rawLang: string): string {
  const key = rawLang.trim().toLowerCase();
  return ALIASES[key] ?? key;
}

export function isSupportedLanguage(rawLang: string): boolean {
  return resolveLanguage(rawLang) in LANG_LOADERS;
}

let highlighterPromise: Promise<HighlighterCore> | undefined;

function getHighlighter(): Promise<HighlighterCore> {
  highlighterPromise ??= createHighlighterCore({
    themes: [
      import('shiki/themes/github-light-default.mjs'),
      import('shiki/themes/github-dark-default.mjs'),
    ],
    langs: [],
    engine: createJavaScriptRegexEngine(),
  });
  return highlighterPromise;
}

const loadedLangs = new Set<string>();

// Highlighting a very long block on every re-render (streaming deltas re-parse
// the whole message) would be wasteful; past this size we skip straight to
// the plain fallback the caller already renders while this resolves.
const MAX_HIGHLIGHT_CHARS = 40_000;

/** Highlight `code` as `rawLang` to an HTML string (a `<pre class="shiki">…`
 * fragment), loading + caching that language's grammar on first use. Returns
 * `undefined` for an unsupported language or an oversized block — the caller
 * renders its own plain fallback in that case. */
export async function highlightCode(code: string, rawLang: string): Promise<string | undefined> {
  if (code.length > MAX_HIGHLIGHT_CHARS) return undefined;
  const lang = resolveLanguage(rawLang);
  const loader = LANG_LOADERS[lang];
  if (loader === undefined) return undefined;
  const highlighter = await getHighlighter();
  if (!loadedLangs.has(lang)) {
    // The per-language dynamic imports are typed as `Promise<unknown>` (a
    // plain lookup table, not shiki's own `LanguageInput`); shiki awaits the
    // module and reads its `default` export either way, so the cast is safe.
    await highlighter.loadLanguage(loader() as never);
    loadedLangs.add(lang);
  }
  return highlighter.codeToHtml(code, { lang, themes: SHIKI_THEMES });
}
