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
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, basicSetup } from 'codemirror';
import { oneDark } from '@codemirror/theme-one-dark';
import { css } from '@codemirror/lang-css';
import { html } from '@codemirror/lang-html';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { python } from '@codemirror/lang-python';
import { rust } from '@codemirror/lang-rust';
import { useTheme } from '@/shared/state/theme';

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

interface CodeEditorProps {
  readonly value: string;
  readonly language: string;
  readonly readOnly: boolean;
  readonly onChange: (next: string) => void;
}

export function CodeEditor({ value, language, readOnly, onChange }: CodeEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

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
        basicSetup,
        ...languageExtension(language),
        ...(dark ? [oneDark] : []),
        EditorView.editable.of(!readOnly),
        EditorState.readOnly.of(readOnly),
        EditorView.theme({ '&': { height: '100%', fontSize: '12px' } }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onChangeRef.current(update.state.doc.toString());
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
