import { defineConfig, devices } from "@playwright/test";

/**
 * Riset R6.4 (plan §7.2, "Visual and a11y"): aplikasi hasil `npm run build` disajikan `vite preview`; API Riset ditiru
 * dari `tests/fixtures/research/canon/*.json` lewat `page.route` (e2e/research/mockApi.ts). Tidak ada backend.
 * Jalankan `npm run build` lebih dulu, lalu `npm run e2e`.
 */
const PORT = 4173;

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.e2e.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    ...devices["Desktop Chrome"],
    // Service worker aplikasi akan melewati `page.route`; layar Riset diuji tanpa SW.
    serviceWorkers: "block",
    reducedMotion: "reduce",
  },
  projects: [
    { name: "chromium" },
    // WebKit (macOS) dengan emulasi iPhone 15: bukan iOS Safari (tanpa papan ketik, bilah URL, VoiceOver). Hanya tes tata letak.
    { name: "webkit-iphone15", testMatch: "**/layout.e2e.ts", use: { ...devices["iPhone 15"] } },
  ],
  webServer: {
    command: `npx vite preview --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
