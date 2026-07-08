import { Suspense } from 'react';
import { CodeView } from '@/features/code/presentation/CodeView';

export default function CodePage() {
  return (
    <Suspense>
      <CodeView />
    </Suspense>
  );
}
