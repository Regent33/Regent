import { Button } from "@/app/ui/Button";

const TITLE = "REGENT";
// Last letter lands at exactly 1.5s: 5 × STAGGER + the 700ms rise below.
const STAGGER = 160;

export function Welcome({
  onNext,
  existingInstall,
  onUninstall,
}: {
  onNext: () => void;
  /** Set when Regent is already on this machine (macOS/Linux) — the quiet
   *  line under the Install button is the only GUI door to uninstalling
   *  there, since nothing like Apps & features exists to open it from. */
  existingInstall?: string | null;
  onUninstall?: () => void;
}) {
  return (
    // pt offset sits inside the centering box, so the block stays centred and
    // just rides lower in the window.
    <div className="mx-auto flex h-full max-w-lg flex-col items-center justify-center pt-16 text-center">
      {/* Fixed size (not vw) so it stays consistent across window sizes and its
          width lines up with the text block below. */}
      <h1
        className="font-display text-[9.5rem] leading-[0.82] tracking-tight text-accent"
        aria-label={TITLE}
      >
        {TITLE.split("").map((letter, i) => (
          <span
            key={i}
            aria-hidden
            // `backwards` holds the letter down and hidden through its delay.
            // Not motion-safe: — see the motion policy in globals.css.
            className="inline-block animate-[letter-rise_700ms_cubic-bezier(0.23,1,0.32,1)_backwards]"
            style={{ animationDelay: `${i * STAGGER}ms` }}
          >
            {letter}
          </span>
        ))}
      </h1>
      <p className="mt-1 text-lg text-text-secondary">Built to serve.</p>
      <p className="mt-2 max-w-md text-sm text-text-tertiary">
        The agent core, the <span className="font-mono">regent</span> CLI, and
        the app — installed in one step.
      </p>
      <Button className="mt-10 px-9 py-2.5 text-base" onClick={onNext}>
        {existingInstall ? "Reinstall" : "Install"}
      </Button>
      {existingInstall && onUninstall && (
        <p className="mt-4 text-xs text-text-tertiary">
          Already installed at{" "}
          <span className="select-text break-all font-mono">{existingInstall}</span>
          {" — "}
          <button
            type="button"
            onClick={onUninstall}
            className="cursor-pointer underline underline-offset-2 hover:text-text-secondary"
          >
            remove it instead
          </button>
        </p>
      )}
    </div>
  );
}
