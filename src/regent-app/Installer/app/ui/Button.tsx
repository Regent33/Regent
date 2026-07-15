import type { ButtonHTMLAttributes } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
}

const variants: Record<Variant, string> = {
  primary: "bg-accent text-on-accent hover:bg-accent-hover shadow-sm",
  secondary:
    "bg-surface text-text-primary border border-stroke-secondary hover:bg-hover",
  ghost: "text-text-secondary hover:text-text-primary hover:bg-hover",
  // A destructive confirm must not wear the same teal as "Install" — the two
  // buttons sit in the same place on screen, and muscle memory is not consent.
  danger: "bg-danger text-on-accent hover:brightness-95 shadow-sm",
};

// Press feedback per emil: transform-only scale on :active, exact transitioned
// properties (never `all`), fast ease-out. Colour changes ride transition-colors.
export function Button({ variant = "primary", className = "", ...rest }: Props) {
  return (
    <button
      className={`inline-flex select-none items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-[transform,background-color,color,filter] duration-150 ease-out active:scale-[0.97] disabled:pointer-events-none disabled:opacity-40 ${variants[variant]} ${className}`}
      {...rest}
    />
  );
}
