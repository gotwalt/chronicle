import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    rollupOptions: {
      external: [
        "react",
        "react/jsx-runtime",
        "react-dom/client",
        "react-router",
        "shiki",
      ],
    },
  },
  server: {
    proxy: {
      "/api": "http://localhost:3000",
    },
  },
});
