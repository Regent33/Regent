import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom';
import '@/app/globals.css';
import { AppShell } from '@/app/presentation/AppShell';
import { HomeClient } from '@/features/chat/presentation/HomeClient';
import { CodeView } from '@/features/code/presentation/CodeView';
import { ArtifactsView } from '@/features/artifacts/presentation/ArtifactsView';
import { CronView } from '@/features/cron/presentation/CronView';
import { MessagingView } from '@/features/messaging/presentation/MessagingView';
import { ProfilesView } from '@/features/profiles/presentation/ProfilesView';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <AppShell>
        <Routes>
          <Route path="/" element={<HomeClient />} />
          <Route path="/code" element={<CodeView />} />
          <Route path="/artifacts" element={<ArtifactsView />} />
          <Route path="/cron" element={<CronView />} />
          <Route path="/messaging" element={<MessagingView />} />
          <Route path="/profiles" element={<ProfilesView />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </AppShell>
    </BrowserRouter>
  </StrictMode>,
);
