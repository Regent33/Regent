'use client';
// Skill detail — skills.view's body rendered through the shared Markdown
// primitive, with tags as a quiet header line.
import { Loader } from '@/shared/ui/Loader';
import { ErrorState } from '@/shared/ui/ErrorState';
import { Markdown } from '@/shared/ui/Markdown';
import { useSkillDetail } from '@/features/skills/viewmodels/useSkillDetail';

export function SkillDetailPane({ name }: { name: string }) {
  const { detail, loading, error } = useSkillDetail(name);

  if (loading) {
    return (
      <div className="p-6">
        <Loader />
      </div>
    );
  }
  if (error !== undefined) return <ErrorState description={error} />;
  if (detail === undefined) return null;

  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-text-primary">{detail.name}</h2>
      {detail.description !== undefined && (
        <p className="mt-1 text-sm text-text-secondary">{detail.description}</p>
      )}
      {detail.tags.length > 0 && (
        <p className="mt-1 text-xs text-text-tertiary">{detail.tags.join(' · ')}</p>
      )}
      <div className="mt-4">
        <Markdown text={detail.body} />
      </div>
    </div>
  );
}
