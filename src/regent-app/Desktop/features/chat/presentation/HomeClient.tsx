'use client';
// Reads the ?id= search param and keys the chat view on it so switching
// sessions remounts with clean state. Without an id, the key falls back to
// the navigation key: re-navigating to a bare `/` (rail button, palette,
// Ctrl/⌘+N) then remounts a fresh chat instead of silently no-opping.
import { useNavigationKey, useSearchParams } from '@/shared/infrastructure/router/adapter';
import { ChatView } from '@/features/chat/presentation/ChatView';

export function HomeClient() {
  const params = useSearchParams();
  const navKey = useNavigationKey();
  const sessionId = params.get('id') ?? undefined;
  return <ChatView key={sessionId ?? navKey} sessionId={sessionId} />;
}
