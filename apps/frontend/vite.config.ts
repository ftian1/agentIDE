import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  clearScreen: false,
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
  },
  build: {
    target: 'esnext',
    minify: 'esbuild',
    // Code-split heavy deps for faster initial load
    rollupOptions: {
      output: {
        manualChunks(id) {
          // xterm is lazy-loaded via React.lazy — keep it separate
          if (id.includes('@xterm')) return 'vendor-xterm';
          // Monaco is lazy-loaded — keep in its own chunk
          if (id.includes('monaco')) return 'vendor-monaco';
        },
      },
    },
    // Shave off bytes from production builds
    sourcemap: false,
  },
}));
