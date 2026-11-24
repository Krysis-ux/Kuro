import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The built assets are served by the Rust daemon from `apps/web/dist`, so paths must be relative to the
// server root.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': { target: 'http://127.0.0.1:8420', changeOrigin: true },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
})
