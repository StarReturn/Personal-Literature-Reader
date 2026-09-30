import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// 开发模式下 /api 代理到本地 axum dev-server；
// Tauri 模式下前端走 IPC，不经过 HTTP。
export default defineConfig({
  plugins: [vue()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8787',
        changeOrigin: true
      }
    }
  },
  build: {
    chunkSizeWarningLimit: 3000
  }
})
