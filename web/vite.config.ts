import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In dev, API calls are proxied to a running `drivesage-web --port 47811`.
export default defineConfig({
  plugins: [react()],
  server: { proxy: { "/api": { target: "http://127.0.0.1:47811", changeOrigin: true } } },
  build: { outDir: "dist", emptyOutDir: true },
});
