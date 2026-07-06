import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { AppShell } from '@/app/presentation/AppShell';
import './globals.css';

export const metadata: Metadata = {
  title: 'Regent',
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en">
      <body>
        <AppShell>{children}</AppShell>
      </body>
    </html>
  );
}
