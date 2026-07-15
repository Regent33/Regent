import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// '@' resolves to this Installer package root, matching the Desktop app's alias
// so shared idioms (`@/app/...`) read identically across both surfaces.
const root = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@': root } },
  build: { outDir: 'out' },
  // Desktop dev-server owns 3000; Setup takes 3100 so both can run at once.
  server: { port: 3100, strictPort: true },
  // Tauri drives the build; keep clear, non-minified errors during bring-up.
  clearScreen: false,
});
