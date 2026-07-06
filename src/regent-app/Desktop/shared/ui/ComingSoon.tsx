// Honest placeholder for routed-but-unbuilt pages: a titled empty state, so
// navigation never lands on something that feels broken.
import { EmptyState } from '@/shared/ui/EmptyState';
import { t } from '@/shared/i18n/t';

export function ComingSoon({ title }: { title: string }) {
  return (
    <div className="flex h-full flex-col">
      <h1 className="px-8 pt-6 text-lg font-semibold text-text-primary">{title}</h1>
      <div className="flex flex-1 items-center justify-center">
        <EmptyState title={title} hint={t().ui.comingSoon} />
      </div>
    </div>
  );
}
