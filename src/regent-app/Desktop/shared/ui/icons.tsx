// The icon set — one inline-SVG component per glyph, stroke = currentColor.
// No icon library; add a glyph here when a surface first needs it.
import type { SVGProps } from 'react';

type IconProps = SVGProps<SVGSVGElement>;

const base = (props: IconProps) => ({
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.75,
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
  className: props.className ?? 'size-4',
  'aria-hidden': true,
  ...props,
});

export const PlusIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const SearchIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="11" cy="11" r="7" />
    <path d="m20 20-3.5-3.5" />
  </svg>
);

export const WrenchIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M14.7 6.3a4.5 4.5 0 0 0-6 5.6L3 17.6V21h3.4l5.7-5.7a4.5 4.5 0 0 0 5.6-6L14.6 12l-2.6-2.6 2.7-3.1Z" />
  </svg>
);

export const MessageIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M21 12a8 8 0 0 1-8 8H4l2.3-2.9A8 8 0 1 1 21 12Z" />
  </svg>
);

export const FileIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M14 3H7a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V7l-4-4Z" />
    <path d="M14 3v4h4" />
  </svg>
);

export const PinIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M12 17v5M7 4h10l-1.5 6.5L18 13H6l2.5-2.5L7 4Z" />
  </svg>
);

export const ChevronDownIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="m6 9 6 6 6-6" />
  </svg>
);

export const AudioIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M11 5 6 9H3v6h3l5 4V5ZM16 9a4 4 0 0 1 0 6M19 7a8 8 0 0 1 0 10" />
  </svg>
);

export const UserIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="12" cy="8" r="4" />
    <path d="M4 21a8 8 0 0 1 16 0" />
  </svg>
);

export const GearIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19 12a7 7 0 0 0-.1-1.2l2-1.6-2-3.4-2.4 1a7 7 0 0 0-2-1.2L14 3h-4l-.4 2.6a7 7 0 0 0-2 1.2l-2.5-1-2 3.4 2 1.6a7 7 0 0 0 0 2.4l-2 1.6 2 3.4 2.5-1a7 7 0 0 0 2 1.2L10 21h4l.4-2.6a7 7 0 0 0 2-1.2l2.5 1 2-3.4-2-1.6c.06-.4.1-.8.1-1.2Z" />
  </svg>
);

export const PanelRightIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <path d="M15 4v16" />
  </svg>
);

export const MinusIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M5 12h14" />
  </svg>
);

export const SquareIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="6" y="6" width="12" height="12" rx="1" />
  </svg>
);

export const CloseIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="m6 6 12 12M18 6 6 18" />
  </svg>
);

export const ButlerIcon = (p: IconProps) => (
  // The Butler Mode mark (assets/ButlerModeIcon.svg) — filled, unlike the
  // stroke set, so it overrides the base fill/stroke.
  <svg
    {...base(p)}
    viewBox="0 0 122.88 116.65"
    fill="currentColor"
    stroke="none"
    fillRule="evenodd"
    clipRule="evenodd"
  >
    <path d="M31.51,106.42V76.94h13.26c5.62,1.01,11.24,4.06,16.86,7.59h10.3c4.66,0.28,7.1,5,2.57,8.11 c-3.61,2.65-8.37,2.5-13.26,2.06c-3.37-0.17-3.51,4.36,0,4.37c1.22,0.1,2.55-0.19,3.7-0.19c6.09-0.01,11.11-1.17,14.19-5.99 l1.54-3.6l15.32-7.59c7.66-2.52,13.12,5.49,7.46,11.07c-11.1,8.07-22.47,14.71-34.11,20.08c-8.45,5.14-16.91,4.96-25.35,0 L31.51,106.42L31.51,106.42L31.51,106.42z M77.89,0c1.92,0,3.66,0.78,4.92,2.04c1.26,1.26,2.04,3,2.04,4.92 c0,0.81-0.14,1.59-0.4,2.31c-0.15,0.43-0.34,0.83-0.57,1.22c1.63,0.24,3.25,0.59,4.85,1.04c1.9,0.53,3.76,1.2,5.56,2.01l0.03,0.02 c7.01,3.16,13.2,8.43,17.49,15.82c3.84,6.61,6.15,14.91,6.16,24.89c0,0.55-0.22,1.05-0.59,1.42l0,0c-0.34,0.34-0.8,0.56-1.3,0.59 l-0.11,0.01l-76.14,0c-0.56,0-1.06-0.22-1.43-0.59l-0.04-0.04c-0.34-0.36-0.55-0.85-0.55-1.39c0-0.06,0-0.11,0.01-0.19 c0.03-9.92,2.35-18.17,6.18-24.74c4.3-7.38,10.49-12.64,17.49-15.79c1.8-0.81,3.66-1.48,5.56-2.01c1.6-0.45,3.22-0.79,4.85-1.04 c-0.23-0.38-0.42-0.79-0.57-1.22c-0.26-0.73-0.4-1.51-0.4-2.31c0.01-1.92,0.79-3.66,2.04-4.92C74.23,0.78,75.97,0,77.89,0L77.89,0z M32.63,66.48V62.3c0-0.33,0.27-0.6,0.6-0.6h89.06c0.33,0,0.6,0.27,0.6,0.6v4.18c0,0.33-0.27,0.6-0.6,0.6H33.22 C32.89,67.08,32.63,66.81,32.63,66.48L32.63,66.48z M41.86,52.24h72.05c-0.3-8.51-2.44-15.59-5.83-21.24 c-3.85-6.43-9.3-11.03-15.44-13.79l-0.03-0.01c-2.3-1.03-4.7-1.81-7.15-2.33c-2.49-0.53-5.03-0.79-7.58-0.79 c-5.06,0-10.11,1.04-14.76,3.13C56.99,19.96,51.54,24.56,47.69,31C44.31,36.65,42.16,43.74,41.86,52.24L41.86,52.24z M53.19,43.19 c-0.17,0.51-0.53,0.92-0.97,1.15c-0.45,0.24-1,0.31-1.53,0.14c-0.53-0.16-0.95-0.53-1.19-0.98c-0.24-0.45-0.31-1-0.14-1.53 c0.72-2.31,1.65-4.43,2.78-6.35c1.13-1.93,2.46-3.66,3.98-5.21c1.51-1.54,3.21-2.89,5.07-4.07c1.86-1.17,3.89-2.17,6.08-2.99 c0.52-0.2,1.07-0.16,1.54,0.05c0.47,0.21,0.86,0.6,1.06,1.12l0.01,0.04c0.18,0.51,0.15,1.04-0.06,1.5 c-0.21,0.47-0.6,0.86-1.12,1.06l-0.03,0.01c-1.92,0.73-3.69,1.59-5.3,2.6c-1.61,1.01-3.08,2.18-4.38,3.5 c-1.29,1.32-2.42,2.79-3.39,4.44c-0.97,1.65-1.77,3.48-2.4,5.49L53.19,43.19L53.19,43.19z M0,74.11h27.52v35.27H0V74.11L0,74.11z" />
  </svg>
);

export const ErrorIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 7v6M12 16.5v.5" />
  </svg>
);

export const PaperclipIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M18.5 10.5 10 19a4 4 0 0 1-5.5-5.8l9-9a3 3 0 0 1 4.5 4l-9 9a2 2 0 1 1-3-2.7l8-8" />
  </svg>
);

export const MicIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="9" y="2" width="6" height="12" rx="3" />
    <path d="M5 10a7 7 0 0 0 14 0M12 21v-3M9 21h6" />
  </svg>
);

export const SendIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <path d="M12 19V5M5 12l7-7 7 7" />
  </svg>
);

export const StopIcon = (p: IconProps) => (
  <svg {...base(p)}>
    <rect x="7" y="7" width="10" height="10" rx="1.5" />
  </svg>
);
