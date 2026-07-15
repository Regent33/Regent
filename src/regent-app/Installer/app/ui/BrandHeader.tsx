// The product mark, repeated on every screen so Setup reads as Regent — the
// one deliberate divergence from Hermes (which drops in-window chrome). Uses
// the real app icon and the Kontes display face.
export function BrandHeader({ subtitle = "Setup" }: { subtitle?: string }) {
  return (
    <div className="flex items-center gap-3">
      <img
        src="/regent-icon.png"
        alt=""
        width={34}
        height={34}
        className="rounded-lg"
        draggable={false}
      />
      <div className="flex items-baseline gap-2">
        <span className="font-display text-2xl leading-none tracking-tight text-text-primary">
          REGENT
        </span>
        <span className="text-sm text-text-tertiary">{subtitle}</span>
      </div>
    </div>
  );
}
