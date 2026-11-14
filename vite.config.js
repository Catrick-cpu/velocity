import { defineConfig } from "vite";

export default defineConfig({
  base: "/velocity/",
  root: "web",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    sourcemap: false,
    chunkSizeWarningLimit: 600,
  },
});
