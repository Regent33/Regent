import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { AppShell } from '@/app/presentation/AppShell';
import './globals.css';

export const metadata: Metadata = {
  title: 'Regent',
};

// Stamp the saved theme onto <html> before first paint so there is no
// light-then-dark flash. Static export ships no data-theme, so without this the
// media default (or light) would paint for a frame before the store applied the
// pick. 'system' (or nothing stored) leaves the attribute off → the media query
// drives. Kept inline and dependency-free; mirrors shared/state/theme.ts.
const noFlashTheme = `try{var m=localStorage.getItem('regent.theme');if(m==='light'||m==='dark')document.documentElement.setAttribute('data-theme',m);}catch(e){}`;

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    // suppressHydrationWarning: the no-flash script sets data-theme on <html>
    // before React hydrates, so the client attribute intentionally differs from
    // the static (attribute-less) SSR markup — that one mismatch is expected.
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: noFlashTheme }} />
      </head>
      <body>
        <AppShell>{children}</AppShell>
      </body>
    </html>
  );
}
