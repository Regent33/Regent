import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// '@' resolves to this Desktop package root, preserving the tsconfig `@/*`
// path alias so no imports elsewhere in the tree have to change.
const root = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': root },
  },
  build: {
    outDir: 'out',
  },
  // Tauri pins the dev server to 3000; fail loudly rather than drift if taken.
  server: {
    port: 3000,
    strictPort: true,
  },
});
