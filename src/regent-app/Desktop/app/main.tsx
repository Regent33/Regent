import { lazy, StrictMode, Suspense } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import '@/app/globals.css';
import { AppShell } from '@/app/presentation/AppShell';

// One lazy chunk per route — the per-page code splitting Next.js did
// implicitly. First paint loads only the shell + the visited view; the
// BootSplash overlays the brief Suspense gap, so fallback stays null.
const lazyView = <T extends object>(load: () => Promise<T>, pick: (m: T) => React.ComponentType) =>
  lazy(() => load().then((m) => ({ default: pick(m) })));

const HomeClient = lazyView(
  () => import('@/features/chat/presentation/HomeClient'),
  (m) => m.HomeClient,
);
const ArtifactsView = lazyView(
  () => import('@/features/artifacts/presentation/ArtifactsView'),
  (m) => m.ArtifactsView,
);
const CronView = lazyView(
  () => import('@/features/cron/presentation/CronView'),
  (m) => m.CronView,
);
const MessagingView = lazyView(
  () => import('@/features/messaging/presentation/MessagingView'),
  (m) => m.MessagingView,
);
const ProfilesView = lazyView(
  () => import('@/features/profiles/presentation/ProfilesView'),
  (m) => m.ProfilesView,
);
const GraphView = lazyView(
  () => import('@/features/graph/presentation/GraphView'),
  (m) => m.GraphView,
);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <AppShell>
        <Suspense fallback={null}>
          <Routes>
            <Route path="/" element={<HomeClient />} />
            <Route path="/artifacts" element={<ArtifactsView />} />
            <Route path="/cron" element={<CronView />} />
            <Route path="/messaging" element={<MessagingView />} />
            <Route path="/profiles" element={<ProfilesView />} />
            <Route path="/graph" element={<GraphView />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Suspense>
      </AppShell>
    </BrowserRouter>
  </StrictMode>,
);
