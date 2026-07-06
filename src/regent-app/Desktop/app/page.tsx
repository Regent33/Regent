import { Suspense } from 'react';
import { HomeClient } from '@/features/chat/presentation/HomeClient';

export default function HomePage() {
  return (
    <Suspense>
      <HomeClient />
    </Suspense>
  );
}
