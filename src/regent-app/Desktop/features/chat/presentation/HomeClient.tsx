'use client';
// Reads the ?id= search param (static export: params are client-side only,
// hence the Suspense boundary in the page) and keys the chat view on it so
// switching sessions remounts with clean state.
import { useSearchParams } from '@/shared/infrastructure/router/adapter';
import { ChatView } from '@/features/chat/presentation/ChatView';

export function HomeClient() {
  const params = useSearchParams();
  const sessionId = params.get('id') ?? undefined;
  return <ChatView key={sessionId ?? 'new'} sessionId={sessionId} />;
}
