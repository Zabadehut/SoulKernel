import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const host = process.env.TAURI_DEV_HOST;
const root = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  // Chemins relatifs pour le bundle embarqué (WebView Tauri)
  base: './',
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
  },
  build: {
    /** Pas de source maps en prod : moins de fichiers servis, pas de fuites de noms dans le bundle. */
    sourcemap: false,
    rollupOptions: {
      input: {
        main: path.resolve(root, 'index.html'),
        hud: path.resolve(root, 'hud.html'),
      },
    },
  },
});
