'use client';
// Right pane — artifacts.get for the selected file: markdown/text through
// the shared Markdown renderer (non-.md text is wrapped in a fenced code
// block so it still gets syntax highlighting), images as a data: URL (the
// backend inlines small images as base64). Anything else — an "other" kind,
// or a text/image file over the backend's inline size cap — falls back to
// metadata + a copy-path button, since the webview has no filesystem access
// and `abs` is informational only.
import { useEffect, useState } from 'react';
import { t } from '@/shared/i18n/t';
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { EmptyState } from '@/shared/ui/EmptyState';
import { Markdown } from '@/shared/ui/Markdown';
import { ZoomableImage } from '@/shared/ui/markdown/ZoomableImage';
import { CopyIcon, CheckIcon, FileIcon } from '@/shared/ui/icons';
import { useArtifactDetail } from '@/features/artifacts/viewmodels/useArtifactDetail';

function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot === -1 ? '' : name.slice(dot + 1).toLowerCase();
}

function asMarkdown(name: string, text: string): string {
  const ext = extensionOf(name);
  if (ext === 'md' || ext === 'markdown') return text;
  return '```' + ext + '\n' + text + '\n```';
}

function CopyPathButton({ path }: { path: string }) {
  const s = t().artifacts;
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(false), 1600);
    return () => clearTimeout(id);
  }, [copied]);

  return (
    <button
      type="button"
      onClick={() => {
        void navigator.clipboard.writeText(path).then(() => setCopied(true));
      }}
      className="inline-flex items-center gap-1.5 rounded-[4px] bg-hover px-2.5 py-1.5 text-xs text-text-secondary transition-colors hover:bg-stroke-secondary hover:text-text-primary"
    >
      {copied ? <CheckIcon className="size-3.5" /> : <CopyIcon className="size-3.5" />}
      {copied ? s.copied : s.copyPath}
    </button>
  );
}

export function ArtifactDetail({ rel, name }: { rel?: string; name?: string }) {
  const s = t().artifacts;
  const { detail, loading, error } = useArtifactDetail(rel);

  if (rel === undefined) {
    return (
      <div className="flex h-full items-center justify-center">
        <EmptyState title={s.selectHint} />
      </div>
    );
  }
  if (loading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader />
      </div>
    );
  }
  if (error !== undefined) return <ErrorState description={error} />;
  if (detail === undefined) return null;

  const textInlined = detail.kind === 'text' && detail.text !== undefined;
  const imageInlined = detail.kind === 'image' && detail.dataBase64 !== undefined;

  return (
    <div className="p-6">
      {textInlined && <Markdown text={asMarkdown(name ?? detail.path, detail.text as string)} />}
      {imageInlined && (
        <ZoomableImage src={`data:${detail.mime};base64,${detail.dataBase64}`} alt={name} />
      )}
      {!textInlined && !imageInlined && (
        <div className="flex flex-col items-start gap-3">
          <FileIcon className="size-6 text-text-tertiary" />
          <p className="text-sm text-text-secondary">{detail.kind === 'other' ? s.noPreview : s.tooLarge}</p>
          <dl className="w-full max-w-md text-xs text-text-tertiary">
            <div className="flex justify-between gap-3 py-1">
              <dt>{s.typeLabel}</dt>
              <dd className="truncate text-text-secondary">{detail.mime}</dd>
            </div>
            <div className="flex justify-between gap-3 py-1">
              <dt>{s.pathLabel}</dt>
              <dd className="truncate text-text-secondary" title={detail.abs}>
                {detail.abs}
              </dd>
            </div>
          </dl>
          <CopyPathButton path={detail.abs} />
        </div>
      )}
    </div>
  );
}
