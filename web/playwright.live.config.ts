import { defineConfig, devices } from "@playwright/test";

/**
 * L4 step 7 (docs/qa/l4/README-live.md): the read-only iPhone 15 spec `e2e-live/iphone.live.ts`, run by Hermes against
 * the LIVE sandbox (real data). Not part of `npm run e2e` or check.sh: they read `playwright.config.ts`, whose
 * `testDir` is `./e2e` and `testMatch` is `**\/*.e2e.ts`; this config has its own `testDir` and `*.live.ts` match.
 * No webServer: the app is already running at LIVE_URL. WebKit with iPhone 15 emulation, so it is the same floor as the
 * mocked suite (not iOS Safari: no keyboard, URL bar, VoiceOver or real safe area).
 *
 * Env (read here, so a missing value fails before any browser starts):
 *   LIVE_URL         origin of investing-app, e.g. https://<mac>.<tailnet>.ts.net:8445 (required)
 *   LIVE_SHOTS_DIR   where screenshots and results.tsv go (required, created if missing)
 *   LIVE_TOKEN       the access token for the unlock screen (read by the spec, never logged)
 * The spec also reads LIVE_URL_FA, LIVE_TOKEN_FA (M17) and LIVE_TICKER_A1, LIVE_TICKER_A2, LIVE_TICKER_BIG.
 */
function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is not set (see docs/qa/l4/README-live.md)`);
  return value;
}

const baseURL = required("LIVE_URL");
required("LIVE_SHOTS_DIR");

export default defineConfig({
  testDir: "./e2e-live",
  testMatch: "**/*.live.ts",
  globalSetup: "./e2e-live/global-setup.ts",
  fullyParallel: false,
  workers: 1,
  forbidOnly: true,
  retries: 0,
  timeout: 240_000,
  expect: { timeout: 15_000 },
  reporter: [["list"]],
  outputDir: "./test-results/live",
  use: {
    baseURL,
    trace: "off",
    screenshot: "off",
    video: "off",
    reducedMotion: "reduce",
    // `page.route` does not see requests a service worker handles, and the read-only guard rests on it: every test blocks
    // the worker except M2 (offline shell), which opts back in and only reads.
    serviceWorkers: "block",
  },
  projects: [{ name: "webkit-iphone15", use: { ...devices["iPhone 15"] } }],
});
