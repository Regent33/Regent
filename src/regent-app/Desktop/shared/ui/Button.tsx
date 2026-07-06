// THE button — one primitive, variants own all chrome (Hermes discipline:
// call sites pass variant/size, never className overrides of padding/color).
import type { ButtonHTMLAttributes } from 'react';

type Variant = 'default' | 'secondary' | 'ghost' | 'text' | 'textStrong';
type Size = 'default' | 'sm' | 'icon' | 'iconSm' | 'iconTitlebar';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

// Text buttons are square (no radius), sized by padding; icon buttons share a
// 4px radius — except the titlebar zone, which is flush and radius-free.
const variants: Record<Variant, string> = {
  default: 'bg-accent text-on-accent hover:bg-accent-hover',
  secondary: 'bg-hover text-text-primary hover:bg-stroke-secondary',
  ghost: 'bg-transparent text-text-secondary hover:bg-hover hover:text-text-primary',
  text: 'bg-transparent text-text-secondary hover:text-text-primary',
  textStrong: 'bg-transparent font-semibold underline underline-offset-2 text-text-primary',
};

const sizes: Record<Size, string> = {
  default: 'px-4 py-2 text-sm',
  sm: 'px-3 py-1.5 text-xs',
  icon: 'p-2 rounded-[4px]',
  iconSm: 'p-1.5 rounded-[4px]',
  iconTitlebar: 'w-[46px] h-full flex items-center justify-center',
};

export function Button({
  variant = 'default',
  size = 'default',
  className = '',
  type = 'button',
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={`inline-flex cursor-pointer select-none items-center justify-center gap-1.5 transition-colors duration-100 disabled:pointer-events-none disabled:opacity-50 ${variants[variant]} ${sizes[size]} ${className}`}
      {...props}
    />
  );
}
