import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { Shell } from '@/features/shell/presentation/Shell';
import './globals.css';

export const metadata: Metadata = {
  title: 'Regent',
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>
        <Shell>{children}</Shell>
      </body>
    </html>
  );
}
