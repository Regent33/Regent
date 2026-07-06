// About — the wordmark + APP_VERSION (no RPC; static build info).
import { APP_VERSION } from '@/app/config/constants';
import { t } from '@/shared/i18n/t';

export function AboutSection() {
  const s = t().settings.about;
  const home = t().home;
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-6 text-center">
      <p className="font-display text-4xl text-accent">{home.wordmark}</p>
      <p className="text-sm text-text-secondary">
        {s.version} {APP_VERSION}
      </p>
    </div>
  );
}
