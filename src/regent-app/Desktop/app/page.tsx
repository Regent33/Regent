import { t } from '@/shared/i18n/t';

export default function HomePage() {
  const strings = t();

  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 text-center">
      <h1
        className="text-6xl font-bold text-accent md:text-8xl"
        style={{ fontFamily: 'var(--font-display)' }}
      >
        {strings.home.wordmark}
      </h1>
      <p className="text-lg text-text-secondary">{strings.home.pitch}</p>
    </div>
  );
}
