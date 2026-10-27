# design.md — Investing App

Status: draft v0 for owner review · Owner: Menganto · Date: 2026-10-27
Requirements: [`prd.md`](prd.md). Build order: [`plan.md`](plan.md).

**Inherited, not repeated here** (copied from `fauzibrevy-hub/finance-app` at `4ef2dd9` in L2 and then owned by this repo):

- Tokens, typography, spacing, radius, shadow, money format, anti-slop rules, accessibility and PWA rules: finance-app `design.md` §1, §6, §7, §9, §10.
- Riset content model, vocabulary, components and display rules: finance-app `docs/research/design.md` §3, §6, §7, §8.
- Company page and every Riset screen block by block at 393 px: finance-app `docs/research/ui-slicing.md` §1–§6 and the decisions in `redesign-brief.md`. Where those documents mention a desktop layout, `#/riset` routes, or the "Riset" bottom tab, this document wins.

UI copy is Indonesian. This document is in English, like the Riset canon.

---

## 1. Principles

1. **Phone first, one column everywhere.** Every screen is a single column: 393 px on the iPhone with an 18 px gutter, centred at max 720 px on the Mac. Same layout on both; no desktop-only screens, sidebars or right panes.
2. **Four places, one tap each.** The bottom bar holds the four places the owner goes to. Nothing important lives behind a "Lainnya" list.
3. **Numbers are never judged by the system.** No verdicts, scores or system fair value. Colour means claim status, data quality, or a chart series.
4. **Every number opens** to its source (`FactDrawer`). **Gaps are shown, never filled.**
5. **The owner's words sit next to the numbers.** Claims, journal and thesis live on the company page, not in a separate tab.
6. **The app is model-free.** Drafts made outside the app arrive as proposals the owner accepts.

## 2. Navigation and routes

### 2.1 Bottom bar

| # | Label | Icon (lucide, stroke 1.75) | Root route | Active for |
|---|---|---|---|---|
| 1 | **Beranda** | `house` | `#/beranda` | `beranda` |
| 2 | **Portofolio** | `chart-pie` | `#/portofolio` | `portofolio` |
| 3 | **Perusahaan** | `building-2` | `#/perusahaan` | `perusahaan`, `perusahaan/:T`, `perusahaan/tambah` |
| 4 | **Alat** | `wrench` | `#/alat` | `alat/*` |

- Four equal slots, **no FAB** (finance-app's FAB records a transaction and does not move). Bar is fixed, floating above content with the finance-app `BottomNav` geometry minus the 72 px centre slot; labels one word, ≤ 10 characters; item ≥ 44 × 44 px.
- Shown at every width, including the Mac (centred with the 720 px column).
- Each tab keeps its own last sub-route and scroll position; tapping the active tab again returns to its root and scrolls to top.
- Pushed screens (company page, Tambah, the three tools) show **‹ Back** (44 px) with the parent's name: "‹ Perusahaan", "‹ Portofolio", "‹ Beranda", "‹ Alat". Back returns to where the owner came from, not always to the tab root (a company page opened from Portofolio goes back to Portofolio).

### 2.2 Route map

```
/  → #/beranda
#/beranda                         Beranda
#/portofolio                      Portofolio
#/perusahaan                      Perusahaan (list)
#/perusahaan/tambah               Tambah emiten (wizard)
#/perusahaan/:T                   Company page   ?section=<block id> scrolls to that block
#/alat                            Alat (list of tools)
#/alat/bandingkan                 Bandingkan     ?pack=<id> | ?ticker=<T>
#/alat/kalibrasi                  Kalibrasi
#/alat/paket                      Paket & aturan ?pack=<id>
```

Hash router, as in finance-app (a path deep link 404s on the server). No redirects from finance-app routes (`#/riset*`, `#/research*`, `#/portfolio`, `#/thesis`): different origin, nothing to redirect.

### 2.3 Where each finance-app screen goes

| finance-app today | investing-app |
|---|---|
| Riset landing block "Perusahaan saya" (`features/riset/screens/Landing.tsx`) | Perusahaan list |
| Riset landing blocks Kotak masuk, Kalender; Desk macro card (`MacroContextCard`) | Beranda |
| Riset landing block "Lainnya" | Alat (the list becomes a tab) |
| Company page `#/riset/:T` (`CompanyPage.tsx`) | `#/perusahaan/:T`, unchanged blocks + new **Tesis** block (§3.4) |
| Bandingkan, Kalibrasi, Paket & aturan, Tambah | `#/alat/*`, `#/perusahaan/tambah`, content unchanged |
| Investasi → Portofolio (`screens/portfolio/PortfolioScreen.tsx`) | Portofolio tab; a position opens its company page |
| Investasi → Tesis (`screens/thesis/ThesisScreen.tsx`) | Tesis block on each company page; draft editor in a `Sheet` |

## 3. Screens (393 px)

App-wide rules: header not sticky (scrolls with content); content `padding-bottom` clears the bar plus safe-area inset; wide tables scroll inside their own box with a sticky first column; "Lipat" = collapsed by default, heading is the toggle; every screen ends with the footer: data age per source · "Bukan saran investasi".

### 3.1 Beranda `#/beranda`

What needs the owner now.

| # | Block | Content | Default | Data |
|---|---|---|---|---|
| 1 | Header | "Beranda" `type-title` · right: **Muat ulang** icon button (44 px) | open | — |
| 2 | Perlu perhatian | Inbox rows, 2 lines each (line 1 `TICKER` · type word · age; line 2 text). Filter chips in one horizontally scrolling row (Semua · Laporan baru · Klaim · KPI · Level · Bendera · Data). First 5, then **Lihat semua**. Tap → company page at the right `?section=`. **Abaikan** inside the opened item | open | `GET /api/research/desk` (inbox) |
| 3 | Kalender | Next 5 items as 2-line rows: date or window · `TICKER` · kind. **Lihat semua** expands to 90 days | open | desk (calendar) |
| 4 | Makro | Indicator cards in a 2-column grid, each 2 lines (name; latest value + signed change). Tap → `Sheet` with the trend chart. No score, regime or lens | open | `GET /api/macro/observations`, `/api/macro/stream` |
| 5 | Footer | Data ages · "Bukan saran investasi" | open | — |

Empty states: "Tidak ada yang perlu perhatian" (inbox), "Tidak ada jadwal 90 hari ke depan" (calendar), "Belum ada data makro · Muat ulang" (macro).

### 3.2 Portofolio `#/portofolio`

Read-only holdings from the portfolio Sheet; content follows finance-app `design.md` §3.3 with these changes: no "Portofolio · Tesis" segmented control, and a position opens its company page.

| # | Block | Content | Data |
|---|---|---|---|
| 1 | Header | "Portofolio" · data age | — |
| 2 | Hero | Market value total; unrealised P/L as one signed line ("+Rp1,2 jt · +4,1%"), the only `Rp` in the block | `GET /api/finance/stock-portfolio` |
| 3 | Ringkasan | 3 two-line stat cards: Modal · Nilai pasar · Kas | same |
| 4 | Komposisi | `DonutChart` by weight, one clearly distinct hue per ticker (never shades of one blue), centre label total and "n emiten"; legend as a list below | same |
| 5 | Posisi | Rows, pitch ≥ 57 px: left `TICKER` + "200 lot · avg 4.150"; right market value + signed P/L %. Tap → `#/perusahaan/:T` | same |
| 6 | Alokasi vs target | `BudgetBar` row per ticker: fill = actual weight, marker = target, right text = drift "+2,1 pp" | same |

Units in headers, numbers right-aligned `tabular-nums`, no repeated `Rp` inside a card.

### 3.3 Perusahaan `#/perusahaan`

| # | Block | Content | Default |
|---|---|---|---|
| 1 | Header | "Perusahaan" · right: **Tambah** (44 px) → `#/perusahaan/tambah` | open |
| 2 | Dipegang | One row per held ticker, 2 lines (pitch 64 px): line 1 `TICKER` · pack label · latest quarter; line 2 claim counts with dots · KPI meter · "Laporan berikut: Q4 26, batas 31 Mar". Tap → company page | open |
| 3 | Manual | Same rows for manual tickers | open if non-empty |
| 4 | Pembanding (n) | Peers, same rows | lipat |
| 5 | Tesis lain (n) | Tickers with a thesis dossier or draft but not in the research universe (prd.md O1): `TICKER` · "Dossier" / "Draft" · updated date | lipat, hidden if empty |
| 6 | Footer | Data ages · "Bukan saran investasi" | open |

Data: `GET /api/research/desk` (coverage rows) and `GET /api/thesis/summary`, `GET /api/thesis/drafts`.

### 3.4 Company page `#/perusahaan/:T`

Blocks 0–10 exactly as finance-app `ui-slicing.md` §1 (Header, Struktur bisnis mind map, Angka kunci, Kuartal terakhir, Membaik / Diawasi, Klaim, Data belum ada, Jurnal, Harga, Keuangan, Footer), with two changes:

- Header back control reads "‹" + the screen the owner came from (§2.1).
- **New block "Tesis"**, between Jurnal (7) and Harga (8), **lipat**:
  - Dossier from the vault, read-only: thesis statement, then Lens, Falsifiers, Review log, Evidence as sub-sections (markdown rendered, lens as label–value rows), "diperbarui <tanggal>".
  - Drafts for this ticker as 2-line rows (title; type · story status · date). Tap → draft detail in a `Sheet` with **Hapus draf** (ghost, `status-over`, confirmation dialog).
  - **Tulis draf tesis** opens the draft editor in a full-height `Sheet` (fields as finance-app `design.md` §3.4; unsent text kept in `localStorage` per ticker; sticky **Simpan draf** 52 px above the keyboard).
  - Empty: "Belum ada tesis untuk <TICKER>" + **Tulis draf tesis**.
  - Data: `GET /api/thesis/{T}`, `GET/POST /api/thesis/drafts`, `DELETE /api/thesis/drafts/{id}`.
  - This is the owner's own written thesis, collapsed below the facts. It does not bring back a thesis line in the header (redesign decision D2 stands).
- Thesis-only state (prd.md O1): header + Tesis block (open) + **Tambah ke riset** (→ `#/perusahaan/tambah?ticker=<T>`).

### 3.5 Alat `#/alat`

| # | Block | Content |
|---|---|---|
| 1 | Header | "Alat" |
| 2 | List | Three rows, ≥ 64 px, each 2 lines (name; one-line purpose): **Bandingkan** "Satu paket, beberapa emiten berdampingan" · **Kalibrasi** "Klaim dan level Anda, dan apa yang terjadi sesudahnya" · **Paket & aturan** "Metrik per industri, sumber pendorong, aturan". Tap → tool screen |
| 3 | Footer | Data ages · "Bukan saran investasi" |

Tool screens keep the content of finance-app `ui-slicing.md` §3 (Bandingkan), §4 (Kalibrasi), §5 (Paket & aturan); Tambah emiten keeps §6 and lives under Perusahaan.

### 3.6 Unlock screen

Same as finance-app's (token once → session cookie), titled "Investasi". During the side-by-side period (prd.md §7 step 1) the sandbox build shows one extra line under the field: "Uji coba: perubahan di sini tidak dibawa ke data asli."

## 4. PWA

| Field | Value |
|---|---|
| `name`, `short_name` | `Investasi` (prd.md O2) |
| `id`, `scope` | `/` |
| `start_url` | `/?source=pwa` → `#/beranda` |
| `display` | `standalone` |
| `theme_color`, `background_color` | `#FFFFFF` |
| `lang` | `id` |
| Icons | 192, 512, maskable 512, apple-touch 180: accent circle with a white "I" (finance-app uses "F"), no gradient |

Service worker: same policy as finance-app (`design.md` §10 there): app shell precached by build hash, `GET /api/*` network-first with 4 s timeout and cached fallback plus the "Offline · data per <waktu>" banner, writes never cached or queued. The app runs on its own origin (`:8445`), so its service worker, Cache Storage and `localStorage` are separate from finance-app's; the session cookie is not (cookies ignore ports), hence the distinct cookie name (prd.md §6).

## 5. Components

Carried over unchanged from finance-app (L2/L3 copy): `Card`, `Segmented`, `Button`, `FormField`, `Sheet`, `SectionHeader`, `Skeleton`, `StatRow`, `DonutChart`, `BudgetBar`, `TrendChart`, `OfflineBanner`, `UpdateBanner`, `UnlockScreen`, and every component the Riset screens import (`FactChip`, `FactDrawer`, `QuarterTable`, `ClaimRow`, `ClaimEditor`, `JournalQuickAdd`, `StructureMindMap`, `HargaCharts`, `KpiEntrySheet`, …).

New or changed:

| Component | Change |
|---|---|
| `BottomNav` | 4 equal slots, no FAB slot; active `accent` + label 700; `<nav aria-label="Navigasi utama">`, `aria-current="page"` |
| `BackHeader` | Label from navigation origin (§2.1), 44 px target |
| `ThesisBlock` | §3.4: dossier sections + drafts list + editor `Sheet`; loading skeleton, empty state, error "Gagal memuat tesis" + Coba lagi |
| `ToolRow` | 2-line row for Alat, ≥ 64 px, chevron |
| `MacroGrid` | The Desk macro card as a 2-column grid of 2-line cards |

Removed (not carried): `Fab`, `DesktopSidebar`, `DetailPanel`, the Investasi segmented control, any desktop-only research component (`RightPane`, desktop Desk, `MobileCoverage`).

## 6. Accessibility

finance-app `design.md` §9 and Riset `design.md` §8 apply, plus:

- Bottom bar: four `<a>` items in a labelled `<nav>`; tab order follows visual order.
- Lipat blocks are `button` headings with `aria-expanded`; the Tesis block's state is announced like the existing blocks.
- Back control has an accessible name that includes its destination ("Kembali ke Portofolio").
- Every route passes axe with no serious or critical violations at 393 px and 200 % text zoom.
