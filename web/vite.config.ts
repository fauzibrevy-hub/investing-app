/// <reference types="vitest/config" />
import { createHash } from "node:crypto";
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import { configDefaults } from "vitest/config";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const root = fileURLToPath(new URL(".", import.meta.url));

/**
 * Membundel src/sw.ts menjadi /sw.js saat `vite build` (design.md §10): daftar precache = semua aset
 * hasil build + file public/, versi cache = hash dari daftar itu. Tidak aktif di `vite dev`.
 */
function investingServiceWorker(): Plugin {
  return {
    name: "investing-service-worker",
    apply: "build",
    enforce: "post",
    async generateBundle(_options, bundle) {
      const built = Object.keys(bundle).filter((f) => !f.endsWith(".map"));
      const publicFiles = readdirSync(`${root}/public`).filter((f) => !f.startsWith("."));
      // index.html = app shell untuk navigasi offline; selalu masuk walau belum ada di bundle saat hook ini jalan.
      const precache = [...new Set([...built, ...publicFiles, "index.html"])].sort().map((f) => `/${f}`);
      const version = createHash("sha256").update(precache.join("\n")).digest("hex").slice(0, 12);
      const result = await build({
        entryPoints: [`${root}/src/sw.ts`],
        bundle: true,
        write: false,
        format: "iife",
        target: "es2020",
        minify: true,
        define: {
          __PRECACHE__: JSON.stringify(precache),
          __SW_VERSION__: JSON.stringify(version),
        },
      });
      this.emitFile({ type: "asset", fileName: "sw.js", source: result.outputFiles[0].text });
    },
  };
}

export default defineConfig({
  plugins: [react(), investingServiceWorker()],
  server: {
    // Hanya untuk `npm run dev`: backend lokal (prd.md §6.3). UI sendiri selalu memanggil path relatif /api/...
    proxy: {
      "/api": { target: "http://127.0.0.1:8787", changeOrigin: false },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    // e2e/ adalah suite Playwright (`npm run e2e`), bukan vitest.
    exclude: [...configDefaults.exclude, "e2e/**"],
  },
});
