import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Dev: `npm run dev` proxies /api to the Rust server on :8080.
export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    proxy: { '/api': 'http://localhost:8080' },
  },
  build: { outDir: 'dist', emptyOutDir: true },
});
