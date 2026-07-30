'use client';
// CodeMirror 6, not Monaco. Monaco was the first choice (it IS VSCode's
// editor), but it does not fit this stack: its editor worker can't be resolved
// under Vite 8 + Rolldown by package path OR by alias, and `import * as monaco`
// drags ~9 MB of language workers into the bundle regardless of how
// MonacoEnvironment is wired. CodeMirror needs no workers at all, is ESM-native,
// and costs roughly a tenth of that for the same job here: read a file, edit it,
// save it. Revisit Monaco only if VSCode-identical behavior is ever the point.
//
// Loaded through WorkspacePanel's React.lazy, so it stays out of the chat bundle.
import { useEffect, useRef } from 'react';
import { EditorState, Prec, type Extension } from '@codemirror/state';
import { EditorView, basicSetup } from 'codemirror';
import { keymap } from '@codemirror/view';
import {
  type CompletionContext,
  type CompletionResult,
  acceptCompletion,
  autocompletion,
} from '@codemirror/autocomplete';
import { indentWithTab } from '@codemirror/commands';
import { oneDark } from '@codemirror/theme-one-dark';
import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { python } from '@codemirror/lang-python';
import { rust } from '@codemirror/lang-rust';
import { useTheme } from '@/shared/state/theme';
import { SELECTION_MAX_CHARS, type EditorSelection } from '@/shared/state/openFile';

/** Language support by the id `languageForPath` produces. Anything unmapped
 * edits as plain text — still fully editable, just unhighlighted. */
function languageExtension(language: string): Extension[] {
  switch (language) {
    case 'typescript':
      return [javascript({ typescript: true, jsx: true })];
    case 'javascript':
      return [javascript({ jsx: true })];
    case 'json':
      return [json()];
    case 'markdown':
      return [markdown()];
    case 'html':
      return [html()];
    case 'css':
    case 'scss':
      return [css()];
    case 'python':
      return [python()];
    case 'rust':
      return [rust()];
    default:
      return [];
  }
}

/** Word completions drawn from the open document. The bundled language modes
 * only complete their own keywords, so without this a Rust or Python file
 * offers nothing at all. Scanning is capped because this runs per keystroke. */
/// Light mode had NO CodeMirror theme, so it fell back to the library default —
/// pure white. Against this app's warm-bone chrome that read as a bright slab
/// cut out of the window, which is the "make the inside a bit darker" report
/// from 2026-07-30.
///
/// Reuses `--surface`, the token panels already use, rather than inventing an
/// editor colour: the editor IS a panel, and a one-off hex here would drift the
/// moment the palette moves. CSS variables resolve normally inside a CodeMirror
/// theme — they are injected as ordinary declarations.
///
/// Dark mode is untouched: oneDark owns it and always did.
const LIGHT_SURFACE = EditorView.theme(
  {
    '&': { backgroundColor: 'var(--surface)' },
    // Gutter matches the text area. A differently-shaded gutter on top of an
    // already-tinted surface reads as a seam rather than a margin.
    '.cm-gutters': {
      backgroundColor: 'var(--surface)',
      borderRight: '1px solid var(--stroke-tertiary)',
      color: 'var(--text-tertiary)',
    },
    // The default active-line wash is near-invisible on a tinted background.
    '.cm-activeLine': { backgroundColor: 'var(--hover)' },
    '.cm-activeLineGutter': { backgroundColor: 'var(--hover)' },
  },
  { dark: false },
);

const WORD_SCAN_MAX_CHARS = 200_000;
const WORD_SUGGESTION_LIMIT = 60;

function documentWords(context: CompletionContext): CompletionResult | null {
  const typed = context.matchBefore(/\w{2,}/);
  if (typed === null || (typed.from === typed.to && !context.explicit)) return null;
  const doc = context.state.doc;
  if (doc.length > WORD_SCAN_MAX_CHARS) return null;
  const words = new Set<string>();
  for (const match of doc.toString().matchAll(/[A-Za-z_$][\w$]{2,}/g)) {
    words.add(match[0]);
    if (words.size > WORD_SUGGESTION_LIMIT * 8) break;
  }
  // Never suggest the fragment the user is mid-way through typing.
  words.delete(typed.text);
  const options = [...words]
    .filter((word) => word.toLowerCase().startsWith(typed.text.toLowerCase()))
    .slice(0, WORD_SUGGESTION_LIMIT)
    .map((label) => ({ label, type: 'text' }));
  return options.length === 0 ? null : { from: typed.from, options };
}

interface CodeEditorProps {
  readonly value: string;
  readonly language: string;
  readonly readOnly: boolean;
  readonly onChange: (next: string) => void;
  /** Reports the highlighted range (undefined when the cursor is just a
   * caret) so a turn can tell the agent which lines the user means. */
  readonly onSelect: (selection: EditorSelection | undefined) => void;
}

export function CodeEditor({ value, language, readOnly, onChange, onSelect }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView>(null);
  // Refs, not deps: these change identity every render, and rebuilding the
  // editor on each one would discard cursor and undo history.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;

  const { mode } = useTheme();
  // CodeMirror takes a concrete theme, so 'system' resolves here rather than
  // deferring to the prefers-color-scheme query the CSS side uses.
  const dark =
    mode === 'dark' ||
    (mode === 'system' &&
      typeof window !== 'undefined' &&
      window.matchMedia?.('(prefers-color-scheme: dark)').matches === true);

  // Rebuild on the inputs that change the editor's configuration (not on every
  // keystroke — `value` is deliberately excluded; the sync effect below handles
  // external content changes without discarding cursor/undo state).
  useEffect(() => {
    if (host.current === null) return;
    const state = EditorState.create({
      doc: value,
      extensions: [
        // basicSetup already brings multi-cursor support, rectangular
        // selection (Alt+drag), undo history, bracket matching/closing, code
        // folding, Ctrl+F search with Ctrl+D select-next-occurrence, Alt+↑/↓
        // move-line, and Mod+/ toggle-comment — the VSCode muscle memory.
        basicSetup,
        // VSCode uses ALT+click to drop an extra cursor; CodeMirror defaults to
        // Ctrl/Cmd+click. Match the editor people actually come from.
        EditorView.clickAddsSelectionRange.of((event) => event.altKey),
        autocompletion({ override: [documentWords], activateOnTyping: true }),
        // Tab accepts the highlighted suggestion, and falls through to indent
        // when the popup isn't open. Prec.highest so it wins over basicSetup's
        // own Tab binding.
        Prec.highest(keymap.of([{ key: 'Tab', run: acceptCompletion }, indentWithTab])),
        ...languageExtension(language),
        ...(dark ? [oneDark] : [LIGHT_SURFACE]),
        EditorView.editable.of(!readOnly),
        EditorState.readOnly.of(readOnly),
        EditorView.theme({ '&': { height: '100%', fontSize: '12px' } }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onChangeRef.current(update.state.doc.toString());
          if (!update.selectionSet && !update.docChanged) return;
          const range = update.state.selection.main;
          if (range.empty) {
            onSelectRef.current(undefined);
            return;
          }
          const doc = update.state.doc;
          const text = update.state.sliceDoc(range.from, range.to);
          onSelectRef.current({
            startLine: doc.lineAt(range.from).number,
            endLine: doc.lineAt(range.to).number,
            text:
              text.length > SELECTION_MAX_CHARS
                ? `${text.slice(0, SELECTION_MAX_CHARS)}\n… (selection trimmed)`
                : text,
          });
        }),
      ],
    });
    const instance = new EditorView({ state, parent: host.current });
    view.current = instance;
    return () => {
      instance.destroy();
      view.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [language, dark, readOnly]);

  // Adopt an externally-changed document (a different file opened, or a reload
  // after the agent edited it) without rebuilding the whole editor.
  useEffect(() => {
    const instance = view.current;
    if (instance === null) return;
    const current = instance.state.doc.toString();
    if (current === value) return;
    instance.dispatch({ changes: { from: 0, to: current.length, insert: value } });
  }, [value]);

  return <div ref={host} className="h-full overflow-auto" />;
}
