// `?inline` bakes the mark into the bundle as a data URI, so it paints with the
// screen instead of fetching after mount — the fetch was late enough to show as
// a visible pop-in. It's the only image we ship, and 9KB, so the bundle can
// carry it. Sized 512px for a 160px render (3x headroom).
import logo from "@/app/assets/regent-logo.webp?inline";

// The brand mark (assets/logo/Regent.png) in its ORIGINAL colour, no background
// tile — black line art, crisp on the light bone theme Setup runs in.
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

// Per-page header — a display-font title (+ optional subtitle). Title-only;
// the OS title bar carries "Regent Setup".
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
        className={`font-display text-2xl leading-none tracking-tight ${
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
