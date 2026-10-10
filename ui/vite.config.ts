import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The window's UI. `pnpm ui:dev` serves it for `tauri dev`; `pnpm ui:build` writes ui/dist.
export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022', sourcemap: false },
  test: { include: ['src/**/*.test.ts'], environment: 'node' },
});
