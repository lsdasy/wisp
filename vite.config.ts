import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Порт и настройки, которые ожидает Tauri (devUrl в tauri.conf.json).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/target/**"] } },
  build: { target: "es2022", sourcemap: false },
});
