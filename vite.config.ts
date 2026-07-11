import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        search: "search.html",
      },
    },
  },
  server: {
    host: "127.0.0.1",
    port: 5174,
    strictPort: false,
  },
});
