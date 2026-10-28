# L4 step 7: iPhone checklist

Two parts. Part 1 is what WebKit emulation ran on the live sandbox (iPhone 15 viewport, real data, 2026-10-28). Part 2 is the rows an emulator cannot cover, still pending on the owner's iPhone 15 (D16).

## 1. Emulated on the live sandbox (WebKit, iPhone 15 viewport, real data)

The spec (`web/e2e-live/iphone.live.ts`, config `web/playwright.live.config.ts`) is read-only. On every company page it blocked the app's `POST /api/research/{T}/seen` (the app marks a company as reviewed when it is opened). The request was stopped, so live data was not changed. Any other write would have failed the row.

Run from `web/` against the sandbox, as in [`README-live.md`](README-live.md). `LIVE_TOKEN` comes from the environment and is never printed or put on a command line:

```sh
export PLAYWRIGHT_BROWSERS_PATH="$HOME/Library/Caches/ms-playwright"
LIVE_URL='https://<mac>.<tailnet>.ts.net:8445' \
LIVE_SHOTS_DIR="$HOME/investing-app-qa/l4-iphone" \
  npx playwright test -c playwright.live.config.ts
```

The collision detector was fixed while running this. `a04e65f` (`web/e2e/audits.ts`) compares text collisions per line box, so a wrapped sentence with inline spans is no longer reported as overlapping its own first line. `d0ecdaa` (M7) waits for the claim list before choosing a card.

This is the full live run after the collide-fix commits (2026-10-28): 10 PASS, 1 SKIP (M17). The table below summarises the spec's `results.tsv` from that run.

The screenshots are named in each row. They show live data, so they are kept locally on the owner's Mac (`~/.hermes/cache/scratch/l4/live-shots`), not in this repository.

| Row | Result | Reason (essentials) | Screenshots (kept locally) |
|---|---|---|---|
| A1 BUMI | PASS | Top, structure map, Keuangan and Harga audited (overflow, text collisions, tap targets, axe, cut digits) | `A1-BUMI-top`, `A1-BUMI-struktur`, `A1-BUMI-struktur-daftar`, `A1-BUMI-keuangan`, `A1-BUMI-harga` |
| A2 BRIS | PASS | No list toggle (empty map). Top, structure map, Keuangan and Harga audited | `A2-BRIS-top`, `A2-BRIS-struktur`, `A2-BRIS-keuangan`, `A2-BRIS-harga` |
| M6 | PASS | Beranda has no filter sheet, so the inbox item and Makro sheets stand in. Body scroll locked while open, restored on close. PageDown stands in for a finger | `M6-inbox-sheet`, `M6-makro-sheet` |
| M7 | PASS | BUMI, the longest of 1 claim card. Content fits at 659, 560 and 420 px high; content scrolls inside the sheet | `M7-claim-normal`, `M7-claim-toolbar`, `M7-claim-short`, `M7-claim-tiny` |
| M8 | PASS | Chip row scrolled 282 of 282 px. Page did not move sideways | `M8-before`, `M8-after` |
| M10 (200 %) | PASS | Root font 200 %. Beranda, company page and all four tabs: no overlap, targets ≥ 44 px. axe not repeated here (A1/A2/M13 cover it at 100 %) | `M10-200-beranda`, `M10-200-company-BUMI`, `M10-200-tab-Beranda`, `M10-200-tab-Perusahaan`, `M10-200-tab-Portofolio`, `M10-200-tab-Alat` |
| M13 | PASS | BUMI: 7 charts in Harga. No cut digits, y-axis labels inside their charts | `M13-ringkasan`, `M13-harga` |
| M15 | PASS | BRIS: back at scroll 600 after the round trip (list was at 45) | `M15-company-scrolled`, `M15-returned` |
| M16 | PASS | BUMI: 11 numbers in the sheet. `?from=portofolio` opened with the Portofolio tab active. Back returned to the list | `M16-sheet`, `M16-list`, `M16-company` |
| M2 (emulated) | PASS | Service worker ready. Beranda offline (1689 chars) and company offline (4436 chars) show app content with the offline banner. Real airplane-mode check still needed (part 2) | `M2-Beranda-offline`, `M2-company-offline` |
| M17 (cookies) | SKIP | `LIVE_URL_FA` and `LIVE_TOKEN_FA` not set. Needs a finance-app login, left to the owner's device | none |

Limits of this emulation are listed in [`README-live.md`](README-live.md) (Safari toolbar, no finger drag, no iOS edge-swipe, no iOS Larger Text).

## 2. Real device (owner, pending)

These rows cannot be covered by WebKit emulation. Copied from [`docs/iphone-checklist.md`](../../iphone-checklist.md). Fill in the `result` column on the iPhone 15 (PASS or FAIL, one line, screenshot).

| id | setup | do | expect | broken looks like | result |
|---|---|---|---|---|---|
| M3 | same app, turn Wi-Fi/cellular off after opening Beranda and a company page, then reload while Tailscale is half-up (e.g. toggle VPN) | pull to refresh, wait > 4 s | data or offline banner | white screen (C2.2 reproduces) | |
| M4 | Safari tab and Home Screen app, company page → Jurnal | tap the textarea with the keyboard up; type 4 lines; hold the keyboard open | the textarea, kind selector and "Simpan" stay visible above the keyboard; page does not zoom; no horizontal pan | composer hidden behind keyboard, page jumps, zoom-in on focus, nav floating over the keyboard | |
| M5 | same | dismiss keyboard (tap outside, then swipe down) | composer returns above the bottom bar, nothing covered | composer stuck mid-screen, nav overlaps "Simpan" | |
| M9 | Landscape (rotate), company page → Jurnal; also Beranda | scroll to top; tap "Simpan"; scroll | composer and "Simpan" fully tappable, not under the bottom bar; safe-area at the notch side respected | "Simpan" half hidden under the nav (C4.3) | |
| M10 | Settings → Display & Brightness → Text Size to largest + Accessibility → Larger Text on; then Safari Aa → 200% | open Beranda, a company page, every bottom-nav tab | all four tabs reachable at largest text and Safari 200%, text wraps, no overlap | tab off-screen (C3.9/C3.10) | |
| M11 | Dark Appearance on, Low Power Mode on, Reduce Motion on | open Beranda, a company page, a sheet | app stays light but readable; no animations, no flicker | unreadable text, status bar white-on-white | |
| M12 | VoiceOver on | swipe through Beranda inbox, open a sheet, then Jurnal composer, Portofolio eye toggle, pushed screen back control, tab bar | four tab names announced, back control "Kembali ke …", eye toggle state announced, Sheets announced as dialogs and focus stays inside until closed, "Tutup" reachable, composer fields labelled, state announced after save | focus escapes behind the sheet, unlabeled buttons, silent save | |
| M14 | Long session | leave the Home Screen app in background 30 min, reopen | session cookie still valid or the unlock screen shows; no white screen | stuck on spinner | |
| M17 | Side-by-side cookies | `finance-app` (`:8443`) and `investing-app` (`:8445`) both installed on Home Screen | unlock one app, use it, switch to the other and reload: unlocking one never locks the other | session cookie collision logs out or locks the other app | |
