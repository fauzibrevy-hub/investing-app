# L4 step 7: the iPhone checklist rows WebKit emulation can cover

`docs/iphone-checklist.md` is the owner's manual list for a real iPhone. Hermes runs the rows below against the **live sandbox** (real data) with a read-only Playwright spec, so the owner only spends time on what a device alone can show.

- Spec: `web/e2e-live/iphone.live.ts`. Config: `web/playwright.live.config.ts` (project `webkit-iphone15`, `devices["iPhone 15"]`, WebKit, no webServer, `workers: 1`, `retries: 0`, trace off).
- Not part of `npm run e2e` or `scripts/check.sh`: those read `web/playwright.config.ts` (`testDir ./e2e`, `*.e2e.ts`). The live spec is `*.live.ts` under `e2e-live/`.
- Type check (not part of `npm run build`, whose tsconfigs do not include `e2e/`): `cd web && npx tsc --noEmit -p e2e-live/tsconfig.json`.

## Run

From the repo root (or any worktree with `web/node_modules`; `cd web && npm ci` once). The browsers come from the shared cache:

```sh
cd web
export PLAYWRIGHT_BROWSERS_PATH="$HOME/Library/Caches/ms-playwright"
LIVE_URL='https://<mac>.<tailnet>.ts.net:8445' \
LIVE_TOKEN="$(cat <path to the investing-app token file>)" \
LIVE_SHOTS_DIR="$HOME/investing-app-qa/l4-iphone" \
  npx playwright test -c playwright.live.config.ts
```

| Env | Required | Meaning |
|---|---|---|
| `LIVE_URL` | yes | Origin of investing-app. The config throws a clear error when it is unset. |
| `LIVE_SHOTS_DIR` | yes | Output directory (created). `results.tsv` is truncated at the start of every run (`globalSetup`), so a re-run of one row with `-g "M16"` also empties it. |
| `LIVE_TOKEN` | yes | Access token for the unlock screen. Typed once per test through `page.evaluate` (never `fill`, whose log prints values), before any screenshot; scrubbed from every message the spec writes. Never put it on a command line. |
| `LIVE_URL_FA`, `LIVE_TOKEN_FA` | M17 only | finance-app origin and token (same host, other port). Without both, M17 is `SKIP`. |
| `LIVE_FA_UNLOCK_PATH` | no | Path of finance-app's unlock POST if it is not `/api/session`. |
| `LIVE_TICKER_A1`, `LIVE_TICKER_A2` | no | Default `BUMI`, `BRIS`. |
| `LIVE_TICKER_BIG` | no | Company with the largest numbers for M7, M13, M2 and M10; default `BUMI`. |

One row: add `-g "M16"` (a title prefix: `A1`, `A2`, `M2`, `M6`, `M7`, `M8`, `M10`, `M13`, `M15`, `M16`, `M17`).

## Output

- `$LIVE_SHOTS_DIR/results.tsv`: one line per row, `<row>\tPASS|FAIL|SKIP\t<one-line reason>`. A PASS line carries notes (what was exercised, what was inconclusive); a FAIL line carries every failed check of that row (checks are soft, so one run lists all of them).
- `$LIVE_SHOTS_DIR/<row>-<step>.png` (viewport, tab bar where it sits on the phone) and `<row>-<step>-full.png` (full page, tab bar hidden for the capture). They show live data; do not commit them. The unlock screen is never shot.
- Paste `results.tsv` into the PR under "Check output" and link the shots, as plan.md §4 asks.

## Read-only guarantee

The only non-GET request that may leave the browser is the unlock `POST /api/session` (and `LIVE_FA_UNLOCK_PATH` for M17). Three layers: a page-side wrapper of `fetch`, `XMLHttpRequest` and `sendBeacon` refuses the write before it leaves the page (this also covers requests a service worker would carry, which `page.route` cannot see, M2); `context.route("**/api/**")` aborts anything that gets past it; a `request` listener flags a write that reached the network. Any of them fails the row: `read-only guard: POST /api/... (stopped)` or `(reached the network)`. The spec only opens and closes sheets; it never presses Abaikan, Muat draf, Simpan, Tutup klaim or Tambah.

**One known exception, decision for the owner:** opening a company page makes the app itself `POST /api/research/<T>/seen` (CompanyPage `markSeen`, the "reviewed" marker behind `TERAKHIR DITINJAU`). The spec stops that request like any write, so live data is untouched, and adds a note to the row (`app tried POST /api/research/BUMI/seen ...: stopped, live data untouched`) instead of failing every company row. Any other write still fails. Dry run: with the original rule (fail on every non-unlock write) A1, A2, M7, M10, M13, M16 and M2 all failed on this request and nothing else was learned. If the owner wants the literal rule, delete `SEEN_MARKER` in the spec. The same marker is why the owner's real-device pass will mark companies as seen.

## Rows covered here, and how

| Row | What the spec does | Limit of the emulation |
|---|---|---|
| A1 BUMI, A2 BRIS | Company page `#/perusahaan/<T>`: top, structure map (and its list view), Keuangan (opened), Harga. Each step: horizontal overflow, text collisions, tap targets ≥ 44 px, axe serious/critical (`e2e/audits.ts`), plus "no digit cut at the left/right edge" (every text node with a digit is inside the viewport, its clipping box, or a horizontal scroller that starts inside the screen; no truncated numbers; SVG axis text included). | Map canvas text is skipped (it pans; nodes are cut at its edge by design). |
| M6 | Beranda: inbox item sheet (closed by ×) and a Makro card sheet (closed by the backdrop). Body scroll lock engaged, page `scrollTop` unchanged by 3 × PageDown while open, body overflow and `scrollTop` restored on close, lowest button above the bottom edge minus the safe-area inset. A control proves PageDown does scroll the page with no sheet open. | **Beranda has no filter sheet** (filters are the chip row, M8), so the Makro sheet stands in. Playwright's mobile WebKit supports neither wheel nor touch drag, so PageDown stands in for a finger; real drag on the dim area and the home-indicator are device checks. Emulated safe-area inset is 0. |
| M7 | BUMI claim sheet with the longest text, at the normal height, 560 px (toolbar shown) and 420 px; if the content never overflowed, also at 300 px. Title and × inside the viewport, content scrolls inside the sheet, header does not move, page does not scroll. | The Safari toolbar is a smaller viewport here, not the real collapsing bar. |
| M8 | Chip row `Filter kotak masuk`: focusing the last chip scrolls the row; page `scrollX` and `scrollingElement.scrollLeft` stay 0, route unchanged, no page overflow. | No finger swipe, no iOS edge-swipe "back" (device). |
| M10 (200 %) | `html{font-size:200% !important}` (as the existing 200 % audit): Beranda, company page and each of the four tabs. Four tabs inside the viewport, each ≥ 44 px, none covered or overlapping (and tapped for real), plus overflow, collisions, targets, cut digits. | Emulates Safari "Aa 200 %" only. iOS Larger Text is a device check. axe is not repeated here. |
| M13 | Largest-number company (`LIVE_TICKER_BIG`): Ringkasan (page top) and Harga: no cut digit, y-axis labels inside their chart, no overflow. | Does not know which ticker is the largest: set `LIVE_TICKER_BIG` if it is not BUMI. |
| M15 | Perusahaan list → scroll 300 → first company → scroll 600 → Portofolio tab → Perusahaan tab: same route, same scroll ±8 px. | Scroll is clamped to the page; a short page is reported in the note (restore not exercised). |
| M16 | Portofolio: first position row → sheet shows ≥ 4 numbers and the Lot, Nilai pasar and P/L labels → **Buka halaman perusahaan** → `#/perusahaan/<T>?from=portofolio`, Portofolio tab active → "‹ Portofolio" → list. | — |
| M2 (emulated) | The one row with the service worker allowed. Loads Beranda and a company page, waits for the worker, reloads online once, then `context.setOffline(true)` and reloads both. PASS needs app content (more than 20 characters and the bottom bar); the note says whether the offline banner showed. FAIL: the browser's own error page (no cached shell) or an empty body. | Playwright WebKit's service worker is not iOS Safari's; the real airplane-mode check stays authoritative. |
| M17 (cookies) | Needs `LIVE_URL_FA` and `LIVE_TOKEN_FA`. One browser context: unlock investing-app, unlock finance-app (its password field, Enter), reload each in turn, finance-app again. Asserts neither falls back to a locked state, `GET /api/session` < 400 on both, `investing_session` present. The report lists cookie **names** per origin (same host, so both appear at both: cookies are not port-scoped), never values. | finance-app's unlock screen is found by its password field; its contract is an assumption (set `LIVE_FA_UNLOCK_PATH` if its unlock path differs). Home Screen installs are device-only. |

## Still needs the real iPhone

M3 (half-up Tailscale, pull to refresh), M4 and M5 (keyboard and composer), M9 (landscape, notch safe area), M10 iOS Larger Text, M11 (Dark Appearance, Low Power Mode, Reduce Motion), M12 (VoiceOver), M14 (30 minutes in the background). Also the finger-only halves of M6 and M8 and the real Safari-toolbar and safe-area parts of M6 and M7.
