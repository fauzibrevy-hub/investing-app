import { defineConfig, devices } from "@playwright/test";

/**
 * D15 (plan §3.2): the built app is served by `vite preview`; the API is mocked with `page.route` from
 * `tests/fixtures/**` and `e2e/fixtures/**` (e2e/mockApi.ts). No backend, no live data.
 * One project, WebKit with iPhone 15 emulation (D13; CI installs WebKit only): not iOS Safari (no keyboard, URL bar
 * or VoiceOver), so it is the automated floor and `docs/iphone-checklist.md` stays the manual device list (D16).
 * Run `npm run build` first, then `npm run e2e`.
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
    // The app's service worker would bypass `page.route`; the screens are tested without it.
    serviceWorkers: "block",
    reducedMotion: "reduce",
  },
  projects: [{ name: "webkit-iphone15", use: { ...devices["iPhone 15"] } }],
  webServer: {
    command: `npx vite preview --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
