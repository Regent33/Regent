// `?inline` bakes the mark into the bundle, so it paints with the screen
// instead of fetching after mount. All Regent app/install icons derive from
// assets/logo/MAIN/icon.png — no alternate logo source.
//
// `regent-mark.png` is assets/logo/Regent.png — the official BACKGROUND-LESS
// logo — cropped to the mark's own bounds and resized to 512 (it ships at
// 7680², and this is inlined as base64: 793 KB became 56 KB). Nothing is
// guessed about which pixels were background, because none of them are.
//
// The tiled `regent-logo.png` bakes a cream rounded slab behind the face,
// which Setup drew as a lighter box floating on its bone-coloured page —
// a frame nothing else on the screen had. That file stays put: it is what the
// OS wants for window, taskbar and shortcut icons, where a mark needs ground.
import logo from "@/app/assets/regent-mark.png?inline";

// The canonical Regent face mark, tile removed so it sits directly on the page.
//
// SIZING NOTE for callers: this art is cropped to the mark's own bounds, so the
// box you give it is filled edge to edge. The tiled asset carried its own
// padding, so the screens paired `h-40` with a NEGATIVE top margin to close the
// gap the tile left — with a bare mark that margin pulled the caption straight
// onto the face. Give it a smaller box (h-28 ≈ the mark's old drawn size inside
// a 160px tile) and ordinary spacing below.
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
