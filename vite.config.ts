import { fileURLToPath, URL } from "node:url";

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      // shadcn ve AI Elements bileşenleri "@/..." ile import ediyor.
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri sabit bir port bekliyor; port doluysa sessizce kaymak yerine düşsün.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // src-tauri'yi izlemek gereksiz yeniden başlatmalara yol açıyor.
      ignored: ["**/src-tauri/**"],
    },
  },
});
