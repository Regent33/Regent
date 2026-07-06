// Faded, gently drifting grid behind the particle core (styles in
// globals.css — the pan animation dies with the global reduced-motion kill).
export function GridBackground() {
  return <div aria-hidden className="butler-grid pointer-events-none absolute inset-0" />;
}
