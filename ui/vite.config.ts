import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The window's UI. `pnpm dev` serves it for `tauri dev`; `pnpm build` writes ui/dist.
export default defineConfig({
  root: __dirname,
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022', sourcemap: false, chunkSizeWarningLimit: 600 },
  test: { include: ['src/**/*.test.ts'], environment: 'node' },
});
