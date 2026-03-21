import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  // 防止 Vite 混淆 Tauri 的 API
  clearScreen: false,
  server: {
    port: 9581,
    host: true,
    strictPort: true,
  },
  // 使用 `TAURI_PLATFORM`、`TAURI_ARCH`、`TAURI_FAMILY`、`TAURI_PLATFORM_VERSION`、`TAURI_PLATFORM_TYPE` 和 `TAURI_DEBUG` 环境变量
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    // Tauri 使用 Chromium 浏览器，支持 ES2021
    target: ['es2021', 'chrome100', 'safari13'],
    // 不为调试构建生成最小化构建
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    // 为调试构建生成源映射
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
