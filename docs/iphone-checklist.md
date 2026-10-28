# iPhone checklist (real device, iPhone 15)

Manual checks from the QA P5 pass (WebKit emulation could not cover them). Not yet observed on a device.

M1 (Home Screen app installed on the v4 build, then deploy) is dropped: the R5–R8 deploy already happened on 2026-10-25.

Setup (once): `investing-app` served over Tailscale HTTPS (`https://<mac>.<tailnet>.ts.net:8445`). Use two modes for each check: **Safari tab** (toolbar shown) and **Home Screen app** (Share → Add to Home Screen, installed as "Investasi"). Note the build by its SW hash: `curl -s https://…/sw.js | head -c 200`.

| id | setup | do | expect | broken looks like | result |
|---|---|---|---|---|---|
| M2 | same app, Airplane mode on, reopen (kill app first) | open Beranda and a company page | either shows old shell with "Offline · data per …" or a clear error; never a white screen | white screen, nav gone | |
| M3 | same app, turn Wi-Fi/cellular off after opening Beranda and a company page, then reload while Tailscale is half-up (e.g. toggle VPN) | pull to refresh, wait > 4 s | data or offline banner | white screen (C2.2 reproduces) | |
| M4 | Safari tab and Home Screen app, company page → Jurnal | tap the textarea with the keyboard up; type 4 lines; hold the keyboard open | the textarea, kind selector and "Simpan" stay visible above the keyboard; page does not zoom; no horizontal pan | composer hidden behind keyboard, page jumps, zoom-in on focus, nav floating over the keyboard | |
| M5 | same | dismiss keyboard (tap outside, then swipe down) | composer returns above the bottom bar, nothing covered | composer stuck mid-screen, nav overlaps "Simpan" | |
| M6 | Beranda, open filter sheet and an inbox item sheet | try to scroll the page behind with a finger (on the dimmed area and on the sheet), then close by drag/backdrop/× | background does not move; body scroll restored on close; sheet's last button clear of the home-indicator bar | background scrolls, sheet jumps, last button under the bar | |
| M7 | Safari tab, toolbar visible then scrolled away | open a tall sheet (claim sheet with long text) while toolbar is shown | header and × visible, content scrolls inside | sheet header off the top of the screen | |
| M8 | Beranda | swipe the filter chips horizontally; start the swipe at the left edge of the screen | chips scroll; page does not move; edge swipe goes back only from the very edge | page scrolls sideways, accidental back navigation | |
| M9 | Landscape (rotate), company page → Jurnal; also Beranda | scroll to top; tap "Simpan"; scroll | composer and "Simpan" fully tappable, not under the bottom bar; safe-area at the notch side respected | "Simpan" half hidden under the nav (C4.3) | |
| M10 | Settings → Display & Brightness → Text Size to largest + Accessibility → Larger Text on; then Safari Aa → 200% | open Beranda, a company page, every bottom-nav tab | all four tabs reachable at largest text and Safari 200%, text wraps, no overlap | tab off-screen (C3.9/C3.10) | |
| M11 | Dark Appearance on, Low Power Mode on, Reduce Motion on | open Beranda, a company page, a sheet | app stays light but readable; no animations, no flicker | unreadable text, status bar white-on-white | |
| M12 | VoiceOver on | swipe through Beranda inbox, open a sheet, then Jurnal composer, Portofolio eye toggle, pushed screen back control, tab bar | four tab names announced, back control "Kembali ke …", eye toggle state announced, Sheets announced as dialogs and focus stays inside until closed, "Tutup" reachable, composer fields labelled, state announced after save | focus escapes behind the sheet, unlabeled buttons, silent save | |
| M13 | Number extremes: a held ticker with large positions (Rupiah in triliun) | open company page Ringkasan and Harga | numbers readable or compacted, y-axis labels fully visible | digits cut at the left/right edge (C5.4) | |
| M14 | Long session | leave the Home Screen app in background 30 min, reopen | session cookie still valid or the unlock screen shows; no white screen | stuck on spinner | |
| M15 | Tab memory | scroll Perusahaan, open a company page, switch to Portofolio and back | returns to the same company page and scroll position | resets to Perusahaan root or loses scroll | |
| M16 | Portofolio position flow | tap a Portofolio position → check Sheet numbers → tap **Buka halaman perusahaan** → tap "‹ Portofolio" | Sheet shows position numbers; button opens `#/perusahaan/:T?from=portofolio` with Portofolio tab active; "‹ Portofolio" returns to the Portofolio list | missing numbers, Perusahaan tab highlighted instead of Portofolio, or back control does not return to list | |
| M17 | Side-by-side cookies | `finance-app` (`:8443`) and `investing-app` (`:8445`) both installed on Home Screen | unlock one app, use it, switch to the other and reload: unlocking one never locks the other | session cookie collision logs out or locks the other app | |

Record result in the `result` column as PASS / FAIL + one line + screenshot (iOS: side button + volume up).
