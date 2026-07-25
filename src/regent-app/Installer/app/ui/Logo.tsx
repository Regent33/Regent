// `?inline` bakes the canonical mark into the bundle, so it paints with the
// screen instead of fetching after mount. All Regent app/install icons derive
// from assets/logo/MAIN/icon.png — no alternate logo source.
import logo from "@/app/assets/regent-logo.png?inline";

// The canonical Regent face mark, including its original cream tile.
export function LogoMark({ className = "h-24 w-24" }: { className?: string }) {
  return (
    <img
      src={logo}
      alt="Regent"
      width={512}
      height={512}
      className={`object-contain ${className}`}
      draggable={false}
    />
  );
}

// Per-page header (+ optional subtitle). Title-only; the OS title bar carries
// "Regent Setup". Body font (Archivo semibold), not the Chorus display face:
// Chorus is a condensed black display type that draws small and tight, which
// reads as a logo at wordmark size but as muddy small print at these header
// sizes. The huge REGENT wordmark on Welcome keeps Chorus; these page headers
// stay in the readable body face.
export function PageHeader({
  title,
  subtitle,
  tone = "default",
}: {
  title: string;
  subtitle?: string;
  tone?: "default" | "danger";
}) {
  return (
    <div>
      <h2
        className={`font-body text-2xl font-semibold leading-tight tracking-tight ${
          tone === "danger" ? "text-danger" : "text-text-primary"
        }`}
      >
        {title}
      </h2>
      {subtitle && (
        <p className="mt-1.5 text-sm text-text-tertiary">{subtitle}</p>
      )}
    </div>
  );
}
