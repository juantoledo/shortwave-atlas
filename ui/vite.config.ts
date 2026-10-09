import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// One bundle for both clients: Tauri loads it from disk, swatlas-server serves it to browsers.
// In `npm run dev`, /api goes to a swatlas-server on :8080 (the Tauri shell uses invoke instead).
export default defineConfig({
  plugins: [react()],
  base: './',
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    proxy: { '/api': { target: 'http://127.0.0.1:8080', ws: true } },
  },
  build: {
    outDir: 'dist',
    target: 'es2022',
    chunkSizeWarningLimit: 2500,
    // flags stay separate files, fetched when shown (inlined, all 270 would land in the bundle)
    assetsInlineLimit: (file: string) => (file.includes('flag-icons') ? false : undefined),
  },
  test: { include: ['src/**/*.test.ts'] },
});
