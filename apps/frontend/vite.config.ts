import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react-swc';
import path from 'path';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      '@repo': path.resolve(__dirname, '../..'),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: 'ws', host, port: 1421 }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
    fs: {
      // Allow imports from the repo root (e.g. pricing.json).
      allow: [path.resolve(__dirname, '../..')],
    },
  },
  build: {
    target: 'esnext',
    minify: 'esbuild',
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('@xterm')) return 'vendor-xterm';
          // monaco-editor is no longer bundled — it's loaded at runtime
          // from /vendor/monaco/ (pre-built AMD, copied by scripts/build-monaco.sh)
        },
      },
    },
    sourcemap: false,
  },
}));
