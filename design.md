# design.md — Investing App

Status: draft v1 for owner review · Owner: Menganto · Date: 2026-10-27
Requirements: [`prd.md`](prd.md). Build order: [`plan.md`](plan.md).

**Inherited, not repeated here** (copied from `fauzibrevy-hub/finance-app` at `4ef2dd9` in L2 and then owned by this repo):

- Tokens, typography, spacing, radius, shadow, money format, anti-slop rules, accessibility and PWA rules: finance-app `design.md` §1, §6, §7, §9, §10.
- Riset content model, vocabulary, components and display rules: finance-app `docs/research/design.md` §3, §6, §7, §8.
- Company page and every Riset screen block by block at 393 px: finance-app `docs/research/ui-slicing.md` §1–§6, its owner decisions in §8, and `redesign-brief.md`. Where those documents mention a desktop layout, `#/riset` or `#/research` routes, or the "Riset" bottom tab, this document wins.

UI copy is Indonesian. This document is in English, like the Riset canon.

**Decisions 2026-10-27** (default chosen by Hermes, owner may override in PR review). They close the QA gaps on navigation and screens and are written into the sections named:

| # | Decision | Section |
|---|---|---|
| D7 | The sandbox line on the unlock screen shows when `GET /api/health` returns `"sandbox": true` | §3.6 |
| D8 | `<HashRouter>` stays; per-tab memory is a hand-rolled store; a pushed screen belongs to the tab it was opened from (`?from=<tab>`) | §2.4 |
| D9 | A Portofolio position opens a Sheet with today's position numbers and **Buka halaman perusahaan** | §3.2 |
| D10 | Vault drafts (no ticker) are listed read-only under Perusahaan "Tesis lain" as "Draf vault (n)" | §3.3 |
| D11 | Hide-amounts stays: eye toggle in the Portofolio header | §3.2 |
| D12 | Beranda Makro reads `GET /api/macro/observations` only; no stream | §3.1 |
| D13 | Mobile only: one 480 px column at every width; desktop branches are deleted, not carried | §1, §2.1, §5 |
| D14 | L3a lands first and owns every file shared between L3 lanes; L3b and L3c branch from `main` after it and run in parallel; closure exceptions instead of carrying desktop files | plan §3.3 |
| D15 | L2/L3 e2e run on `vite preview` with `page.route` mocks only, in the `webkit-iphone15` project; real held tickers are checked at L4 | plan §3.2, §4 |
| D16 | Phone floor is split: automated Playwright WebKit iPhone 15 floor on every route; `docs/iphone-checklist.md` is the owner's manual device list | §6, plan §3.2 |

---

## 1. Principles

1. **Phone first, one column everywhere.** Every screen is a single column: 393 px on the iPhone with an 18 px gutter (`--gutter-left/right`, finance-app `tokens.css:89`), centred at max 480 px on the Mac (`max-w-mobile`, finance-app `tailwind.config.ts:101`). Same layout on both; no desktop-only screens, sidebars or right panes, and no `≥ 1024 px` variants in carried components (D13).
2. **Four places, one tap each.** The bottom bar holds the four places the owner goes to. Nothing important lives behind a "Lainnya" list.
3. **Numbers are never judged by the system.** No verdicts, scores or system fair value. Colour means claim status, data quality, or a chart series.
4. **Every number opens** to its source (`FactDrawer`). **Gaps are shown, never filled.**
5. **The owner's words sit next to the numbers.** Claims, journal and thesis live on the company page, not in a separate tab.
6. **The app is model-free.** Drafts made outside the app arrive as proposals the owner accepts.

## 2. Navigation and routes

### 2.1 Bottom bar

| # | Label | Icon (lucide, stroke 1.75) | Root route | Active for |
|---|---|---|---|---|
| 1 | **Beranda** | `house` | `#/beranda` | `beranda`; any pushed screen with `from=beranda` |
| 2 | **Portofolio** | `chart-pie` | `#/portofolio` | `portofolio`; any pushed screen with `from=portofolio` |
| 3 | **Perusahaan** | `building-2` | `#/perusahaan` | `perusahaan`, `perusahaan/tambah`, `perusahaan/:T` with no `from` or `from=perusahaan` |
| 4 | **Alat** | `wrench` | `#/alat` | `alat`, `alat/*`; any pushed screen with `from=alat` |

- Four equal slots (`grid-cols-4`), **no FAB** (finance-app's FAB records a transaction and does not move). Bar is fixed, floating above content with the finance-app `BottomNav` geometry minus the 72 px centre slot; labels one word, ≤ 10 characters; item ≥ 44 × 44 px.
- Shown at every width, including the Mac: bar width `min(100vw, 480px) − 30px`, centred with the column (finance-app `AppShell.tsx:88`).
- Tab memory, scroll and back behaviour: §2.4.
- Pushed screens (company page, Tambah, the three tools) show **‹ <owner tab>** (44 px): "‹ Perusahaan", "‹ Portofolio", "‹ Beranda", "‹ Alat".

### 2.2 Route map

```
/  → #/beranda
#/beranda                         Beranda
#/portofolio                      Portofolio
#/perusahaan                      Perusahaan (list)
#/perusahaan/tambah               Tambah emiten (wizard)   ?ticker=<T> prefills the code
#/perusahaan/:T                   Company page   ?section=<block id> scrolls to that block
#/alat                            Alat (list of tools)
#/alat/bandingkan                 Bandingkan     ?pack=<id> | ?ticker=<T>
#/alat/kalibrasi                  Kalibrasi
#/alat/paket                      Paket & aturan ?pack=<id>
any pushed route                  &from=beranda|portofolio|perusahaan|alat (§2.4)
```

Hash router, as in finance-app (a path deep link 404s on the server). No redirects from finance-app routes (`#/riset*`, `#/research*`, `#/portfolio*`, `#/thesis*`, `#/idx*`): different origin, nothing to redirect. Unknown routes go to `#/beranda`.

### 2.3 Where each finance-app screen goes

| finance-app today | investing-app |
|---|---|
| Riset landing block "Perusahaan saya" (`features/riset/screens/Landing.tsx`) | Perusahaan list |
| Riset landing blocks Kotak masuk, Kalender, Makro (`MacroContextCard`) | Beranda |
| Riset landing block "Lainnya" | Alat (the list becomes a tab) |
| Company page `#/riset/:T` (`CompanyPage.tsx`) | `#/perusahaan/:T`, unchanged blocks + new **Tesis** block (§3.4) |
| Bandingkan, Kalibrasi, Paket & aturan | `#/alat/*`, content unchanged |
| Tambah `#/riset/add` | `#/perusahaan/tambah`, content unchanged plus `?ticker=` prefill (§3.4) |
| Investasi → Portofolio (`screens/portfolio/PortfolioScreen.tsx`, mobile variant) | Portofolio tab; a position opens a Sheet (§3.2) |
| Investasi → Portofolio position screen `#/portfolio/:ticker` (`PositionDetail`) | Position Sheet on Portofolio (§3.2); no route |
| Investasi → Tesis (`screens/thesis/ThesisScreen.tsx`, mobile variant) | Tesis block on each company page; thesis items outside the research universe and vault drafts under Perusahaan "Tesis lain" (§3.3); draft editor in a `Sheet` |
| Desktop variants (`PortfolioDesktop`, `ThesisDesktop`, Desk, Dossier, `DetailPanel`, `DesktopSidebar`) | Not carried (D13) |

### 2.4 Navigation state (D8)

New code in L2 (`web/src/shell/navState.ts` + `BackHeader`); finance-app has none of it.

1. **Router.** `<HashRouter>` stays (react-router-dom 6.30.1). React Router's `<ScrollRestoration>` needs a data router (`createHashRouter`), so it is not used; the store below replaces it.
2. **Store.** In memory, not persisted: `lastHash: Record<Tab, string>` and `scrollY: Map<hash, number>` (at most 50 entries, oldest dropped). A cold start opens the deep link or `#/beranda` with empty memory.
3. **Owner tab.** `owner(location)` = the `from` query value when it names a tab, else the route's own tab (§2.1 table). The owner tab is the one highlighted, and the pushed screen is written to the owner's `lastHash`. A company page opened from Portofolio is `#/perusahaan/BUMI?from=portofolio`: Portofolio stays highlighted and Perusahaan's memory is not touched.
4. **Links that push across tabs carry `from`.** Beranda inbox and calendar rows, Portofolio's **Buka halaman perusahaan**, Bandingkan and Kalibrasi ticker links add `from=<their tab>`. Pushes inside a tab (Perusahaan list → company page, Alat → tool) may omit it.
5. **On every location change.** Save `window.scrollY` under the hash being left, set `lastHash[owner] = new hash`, render, then on the next animation frame scroll to the saved `scrollY` for the new hash (0 if none). Screens read through the `useResource` cache, so a revisited screen renders at full height before the restore.
6. **Tap on another tab.** Go to that tab's `lastHash` (its root if empty).
7. **Tap on the active tab.** Go to the tab's root, scroll to 0, forget the root's saved `scrollY`.
8. **Back control.** Label "‹ " + owner tab label; accessible name "Kembali ke <owner tab>". Action: `navigate(-1)` only when the previous history entry (`history.state.idx - 1`) was rendered in this app with the same owner tab (the store records each entry's owner by `idx`). Otherwise (a cold deep link, or a pushed screen reached again through tab memory after a tab tap) navigate with `replace` to the owner tab's stored route: the last root-level hash visited in that tab, else its root. Scroll comes back through rule 5. Decision 2026-10-27 (Hermes default, owner may override in review).
9. **Cold deep link** (no `from`, no in-app history): the route's own tab owns it, so a company page shows "‹ Perusahaan" and goes to `#/perusahaan`; a tool shows "‹ Alat".
10. **`?section=`** still scrolls to the block (finance-app `CompanyPage.tsx:49`) and wins over a saved `scrollY` on the first render of that hash.

Tests (L2): unit tests for the pure store (`owner`, save/restore, cap); Playwright `nav.e2e.ts` (plan §4 L2) for rules 3, 5, 6, 7, 8, 9.

## 3. Screens (393 px)

App-wide rules: header not sticky (scrolls with content); content `padding-bottom` clears the bar plus safe-area inset; wide tables scroll inside their own box with a sticky first column; "Lipat" = collapsed by default, heading is the toggle; every screen ends with the footer: data age per source · "Bukan saran investasi".

### 3.1 Beranda `#/beranda`

What needs the owner now.

| # | Block | Content | Default | Data |
|---|---|---|---|---|
| 1 | Header | "Beranda" `type-title` · right: **Muat ulang** icon button (44 px) | open | — |
| 2 | Perlu perhatian | Inbox rows, 2 lines each (line 1 `TICKER` · type word · age; line 2 text). Filter chips in one horizontally scrolling row (Semua · Laporan baru · Klaim · KPI · Level · Bendera · Data). First 5, then **Lihat semua**. Tap → company page at the right `?section=` with `from=beranda`. **Abaikan** inside the opened item | open | `GET /api/research/desk` (inbox) |
| 3 | Kalender | Next 5 items as 2-line rows: date or window · `TICKER` · kind. **Lihat semua** expands to 90 days | open | desk (calendar) |
| 4 | Makro | Indicator cards in a 2-column grid, each 2 lines (name; latest value + signed change). Tap → `Sheet` with the trend chart. No score, regime or lens. Values load on open and on **Muat ulang**; no live stream (finance-app `docs/research/prd.md:169`, R7.4; D12) | open | `GET /api/macro/observations` only (as finance-app `useMacro.ts:24`) |
| 5 | Footer | Data ages · "Bukan saran investasi" | open | — |

Empty states: "Tidak ada yang perlu perhatian" (inbox), "Tidak ada jadwal 90 hari ke depan" (calendar), "Belum ada data makro · Muat ulang" (macro).

### 3.2 Portofolio `#/portofolio`

Read-only holdings from the portfolio Sheet. Content follows finance-app `design.md` §3.3 (the mobile variant of `PortfolioScreen.tsx`) with these changes: no "Portofolio · Tesis" segmented control (`InvestTabs`), no desktop variant (D13), a position opens a Sheet (D9), and the hide-amounts toggle moves here (D11).

| # | Block | Content | Data |
|---|---|---|---|
| 1 | Header | "Portofolio" · data age · right: eye toggle (44 px, `aria-pressed`, name "Sembunyikan nominal" / "Tampilkan nominal", as finance-app `HomeScreen.tsx:67`). On: every `Rp` value in the app shows `Rp•••` (`useHideAmounts`, kept in `localStorage`) | — |
| 2 | Hero | Market value total; unrealised P/L as one signed line ("+Rp1,2 jt · +4,1%"), the only `Rp` in the block | `GET /api/finance/stock-portfolio` |
| 3 | Ringkasan | 3 two-line stat cards: Modal · Nilai pasar · Kas | same |
| 4 | Komposisi | `DonutChart` by weight, one clearly distinct hue per ticker (never shades of one blue), centre label total and "n emiten"; legend as a list below | same |
| 5 | Posisi | Rows, pitch ≥ 57 px: left `TICKER` + "200 lot · avg 4.150"; right market value + signed P/L %. Tap → position Sheet (below) | same |
| 6 | Alokasi vs target | `BudgetBar` row per ticker: fill = actual weight, marker = target, right text = drift "+2,1 pp" | same |
| 7 | Footer | Data age · "Bukan saran investasi" | — |

Units in headers, numbers right-aligned `tabular-nums`, no repeated `Rp` inside a card.

**Position Sheet** (D9). Bottom `Sheet`, title `TICKER`. Body = finance-app `PositionDetail` (`PortfolioScreen.tsx:208-229`) unchanged: company name and category, then label–value rows Lot · Lembar · Harga rata-rata · Harga close · Modal · Nilai pasar · P/L (Rp, signed) · P/L % · Bobot · Target · Drift. Below them one secondary button, full width, 44 px: **Buka halaman perusahaan** → `#/perusahaan/:T?from=portofolio`. finance-app's links "Dossier tesis", "Riset" and "Matriks XBRL" are dropped (the company page holds research and thesis). Every number today's position screen shows stays reachable.

### 3.3 Perusahaan `#/perusahaan`

| # | Block | Content | Default |
|---|---|---|---|
| 1 | Header | "Perusahaan" · right: **Tambah** (44 px) → `#/perusahaan/tambah` | open |
| 2 | Dipegang | One row per held ticker, 2 lines (pitch 64 px): line 1 `TICKER` · pack label · latest quarter; line 2 claim counts with dots · KPI meter · "Laporan berikut: Q4 26, batas 31 Mar". Tap → company page | open |
| 3 | Manual | Same rows for manual tickers | open if non-empty |
| 4 | Pembanding (n) | Peers, same rows | lipat |
| 5 | Tesis lain (n) | Tickers with a thesis dossier or draft but not in the research universe (prd.md O1): `TICKER` · "Dossier" / "Draf" · updated date; tap → thesis-only company page (§3.4). Last row, when vault drafts exist: **Draf vault (n)** (D10), tap → vault drafts Sheet | lipat, hidden if both lists are empty |
| 6 | Footer | Data ages · "Bukan saran investasi" | open |

**Vault drafts Sheet** (D10). Vault drafts have no ticker (finance-app `screens/thesis/types.ts:58-67`; `ThesisScreen.tsx:59-60` files them under "Vault"). The Sheet lists them as 2-line rows (title; `status_lifecycle` · file name). Tap → read-only draft detail in the same Sheet (statement, falsifiers, file name); no **Hapus** (the backend refuses to delete a vault draft).

Data: `GET /api/research/desk` (coverage rows), `GET /api/thesis/summary` and `GET /api/thesis/drafts` (Tesis lain; `vault_drafts.length` gives n), `GET /api/thesis/drafts/vault` (vault drafts Sheet, loaded on open), `GET /api/thesis/drafts/{id}` (draft detail).

### 3.4 Company page `#/perusahaan/:T`

Blocks 0–10 exactly as finance-app `ui-slicing.md` §1 (Header, Struktur bisnis mind map, Angka kunci, Kuartal terakhir, Membaik / Diawasi, Klaim, Data belum ada, Jurnal, Harga, Keuangan, Footer), with these changes:

- Header back control per §2.4 ("‹ <owner tab>").
- `FactDrawer` uses its phone variant at every width; its `≥ 1024 px` branch is deleted (D13).
- **New block "Tesis"**, between Jurnal (7) and Harga (8), **lipat**, block id `tesis` (`?section=tesis`, alias `thesis`, opens it and scrolls to it; plan §3.3):
  - Dossier from the vault, read-only: thesis statement, then Lens, Falsifiers, Review log, Evidence as sub-sections (markdown rendered, lens as label–value rows), "diperbarui <tanggal>".
  - Drafts for this ticker (rows of `GET /api/thesis/drafts` whose `ticker` is T) as 2-line rows (title; type · story status · date). Tap → draft detail in a `Sheet` with **Hapus draf** (ghost, `status-over`, confirmation dialog).
  - **Tulis draf tesis** opens the draft editor in a full-height `Sheet` (fields as finance-app `design.md` §3.4; unsent text kept in `localStorage` per ticker; sticky **Simpan draf** 52 px above the keyboard).
  - Empty: "Belum ada tesis untuk <TICKER>" + **Tulis draf tesis**.
  - Loading skeleton; error "Gagal memuat tesis" + **Coba lagi**.
  - Data: `GET /api/thesis/{T}`, `GET/POST /api/thesis/drafts`, `GET/DELETE /api/thesis/drafts/{id}`.
  - This is the owner's own written thesis, collapsed below the facts. It does not bring back a thesis line in the header: finance-app `docs/research/ui-slicing.md` §8 D2 ("no thesis", decided 2026-10-25) stands. (That is not finance-app `docs/research/plan.md` D2, which is about desktop-first.)
- Thesis-only state (prd.md O1): header + Tesis block (open) + **Tambah ke riset** (→ `#/perusahaan/tambah?ticker=<T>`).

**Tambah emiten** `#/perusahaan/tambah` keeps finance-app `ui-slicing.md` §6. Its one change: `?ticker=<T>` prefills the code field and runs the lookup once (finance-app `features/riset/screens/Add.tsx` reads no search params today). Nothing else in Add changes.

### 3.5 Alat `#/alat`

| # | Block | Content |
|---|---|---|
| 1 | Header | "Alat" |
| 2 | List | Three rows, ≥ 64 px, each 2 lines (name; one-line purpose): **Bandingkan** "Satu paket, beberapa emiten berdampingan" · **Kalibrasi** "Klaim dan level Anda, dan apa yang terjadi sesudahnya" · **Paket & aturan** "Metrik per industri, sumber pendorong, aturan". Tap → tool screen |
| 3 | Footer | Data ages · "Bukan saran investasi" |

Tool screens keep the content of finance-app `ui-slicing.md` §3 (Bandingkan), §4 (Kalibrasi), §5 (Paket & aturan); Tambah emiten lives under Perusahaan (§3.4).

### 3.6 Unlock screen

Same as finance-app's (token once → session cookie), titled "Buka kunci Investasi". During the side-by-side period (prd.md §7 step 1) it shows one extra line under the field: "Uji coba: perubahan di sini tidak dibawa ke data asli." The line shows only when `GET /api/health` (the one open route) returns `"sandbox": true`, which the backend sets when `INVESTING_SANDBOX=1` (plan §3.1, D7). The L4 env file sets it, L5 removes it; the same `web/dist` serves both. If the health call fails the line stays hidden.

## 4. PWA

| Field | Value |
|---|---|
| `name`, `short_name` | `Investasi` (prd.md O2) |
| `id`, `scope` | `/` |
| `start_url` | `/?source=pwa` → `#/beranda` |
| `display` | `standalone` |
| `theme_color`, `background_color` | `#FFFFFF` |
| `lang` | `id` |
| Icons | 192, 512, maskable 512, apple-touch 180: accent circle with a white "I" (finance-app uses "F"), no gradient. Generated by `npm run icons` after the glyph function `inF` in `web/scripts/gen-icons.mjs:21` is replaced by an "I" (one centred bar, 0.24 of the glyph box wide); the PNGs are committed |
| `favicon.svg` | Same circle with an "I" path, e.g. `<path fill="#FFF" d="M28.2 17.3h7.6v29.4h-7.6z"/>` in place of the "F" path |
| HTML | `<title>Investasi</title>`, `apple-mobile-web-app-title` `Investasi` (`web/index.html`) |

Service worker: same policy as finance-app (`design.md` §10 there): app shell precached by build hash, `GET /api/*` network-first with 4 s timeout and cached fallback plus the "Offline · data per <waktu>" banner, writes never cached or queued. Cache names `investing-api-*`, `investing-shell-*`; the cached-response date header is `X-Investing-Cache-Date`. The app runs on its own origin (`:8445`), so its service worker, Cache Storage and `localStorage` are separate from finance-app's; the session cookie is not (cookies ignore ports), hence the distinct cookie name (prd.md §6).

## 5. Components

Carried over unchanged from finance-app (L2/L3 copy): `Card`, `Segmented`, `Button`, `FormField`, `Sheet`, `SectionHeader`, `Skeleton`, `StatRow`, `DataTable`, `TransactionRow` (its row and `ListState` are used by Portofolio and the thesis views), `Money`, `DonutChart`, `BudgetBar`, `TrendChart`, `PriceChart`, `ErrorBoundary`, `OfflineBanner`, `UpdateBanner`, `UnlockScreen`, and every component the Riset screens import (`FactChip`, `FactDrawer`, `QuarterTable`, `ClaimRow`, `ClaimEditor`, `JournalQuickAdd`, `StructureMindMap`, `HargaCharts`, `KpiEntrySheet`, …). `DonutChart`, `TrendChart` and `FactDrawer` lose their `useIsDesktop` branch and always take the mobile size (D13).

New or changed:

| Component | Change |
|---|---|
| `AppShell` | Mobile layout only: `max-w-mobile` column + bar at every width; no `useShell`/`ShellContext`, no `isDesktop`, no `openDetail` (D13) |
| `BottomNav` | 4 equal slots, no FAB slot; active `accent` + label 700; `<nav aria-label="Navigasi utama">`, `aria-current="page"` on the owner tab (§2.4) |
| `BackHeader` | "‹ <owner tab>", 44 px target, name "Kembali ke <owner tab>" (§2.4) |
| `PositionSheet` | §3.2: `PositionDetail` + **Buka halaman perusahaan** |
| `DetailStat` | Label–value row of `PositionDetail`, moved from finance-app `screens/home/TxParts.tsx:84` into `components/DetailStat.tsx` |
| `ThesisBlock` | §3.4: dossier sections + drafts list + editor `Sheet`; loading skeleton, empty state, error "Gagal memuat tesis" + Coba lagi |
| `VaultDraftsSheet` | §3.3: read-only list and detail of vault drafts |
| `ToolRow` | 2-line row for Alat, ≥ 64 px, chevron |
| `MacroGrid` | The Desk macro card as a 2-column grid of 2-line cards |

Removed (not carried): `Fab`, `DesktopSidebar`, `DetailPanel`, `useShell`, the Investasi segmented control (`InvestTabs`), `PortfolioDesktop`, `ThesisDesktop`, `TxParts`, `AccountCard`, `Avatar`, the dev component catalog, and every desktop-only research screen (`DeskScreen`, `MobileDesk`, `MobileCoverage`, `CoverageBoard`, `desk/Inbox`, `desk/Calendar`, `dossier/*`, `RightPane`). The few symbols carried screens import from them are moved, not copied with their files (plan §3.3).

## 6. Accessibility

finance-app `design.md` §9 and Riset `design.md` §8 apply, plus:

- Bottom bar: four `<a>` items in a labelled `<nav>`; tab order follows visual order.
- Lipat blocks are `button` headings with `aria-expanded`; the Tesis block's state is announced like the existing blocks.
- Back control has an accessible name that includes its destination ("Kembali ke Portofolio").
- The eye toggle is a `button` with `aria-pressed`; the position and vault-draft Sheets are dialogs that trap focus and return it to the row that opened them.
- **Automated floor** (D16): Playwright WebKit with the iPhone 15 descriptor (393 × 852) on every route in §2.2: no page sideways scroll, every target ≥ 44 px, axe with no serious or critical violations, also at 200 % root font size. Plan §4 L2/L3.
- **Manual floor:** `docs/iphone-checklist.md` on a real iPhone 15 (keyboard, VoiceOver, Airplane mode, landscape, largest text), run by the owner at L4.
