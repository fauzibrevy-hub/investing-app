# plan.md — Investing App

Status: draft v0 for owner review · Owner: Menganto · Date: 2026-10-27
Requirements: [`prd.md`](prd.md). Screens: [`design.md`](design.md).

## 1. Approach

- **Clean copy, not a fork.** Code is copied from a clean checkout of `fauzibrevy-hub/finance-app` at a pinned commit (`4ef2dd9` unless the owner names a later one at L1 start), by import closure, into this repo with fresh history. Each copying PR states the source commit. finance-app's history stays in finance-app.
- **Move first, change nothing.** L1–L3 change names, routes and navigation only. Screen content, formulas and endpoint JSON stay byte-for-byte what finance-app serves, so parity is a diff.
- **finance-app is untouched until L6.** It stays the live app and the live research writer through the side-by-side period.
- **Who does what.** Hermes writes briefs, dispatches, and checks each slice itself before releasing the next. Code is written by Claude Code (Sonnet lanes, Opus for review leads). Every slice is a **draft PR** into `main`; the owner reviews and merges. Commits as Menganto via env vars, never a Co-Authored-By trailer, never touching git config, never pushing to `main` directly after L0.
- **Live data rule.** Lanes never touch `~/finance-app-data`, `~/investing-app-data`, `:8787` or `:8789`/`:8445` except where a node below says so after the owner's OK. Sandboxes use APFS clones (`cp -c -R`) and throwaway tokens.

## 2. Slices

| # | Slice | Contents | Done when |
|---|---|---|---|
| L0 | Canon | `prd.md`, `design.md`, `plan.md`, `README.md`, `CLAUDE.md` | Owner merges the canon PR |
| L1 | Backend move | §3.1 | §4 L1 checks green |
| L2 | Web shell | §3.2 | §4 L2 checks green; owner answers prd.md O2 |
| L3a | Beranda + Alat | §3.3 | §4 L3 checks green for these routes |
| L3b | Perusahaan + company page + Tesis block | §3.3 | §4 L3 checks green; owner answers prd.md O1 |
| L3c | Portofolio | §3.3 | §4 L3 checks green |
| L4 | Side by side | §3.4 | `parity.sh` and `counts.sh` 0 differences; iPhone checklist passes; owner uses it alongside finance-app |
| L5 | Cutover | §3.5 | Owner's go; switch steps done; health 200 on `:8789` and `:8445`; one-hour watch clean |
| L6 | Removal from finance-app | §3.6 | finance-app PR merged by the owner; prd.md S6 |

L1 → L2 → L3a/L3b/L3c (parallel lanes, stacked on L2) → L4 → L5 → L6. Owner gates: every PR merge, the L4 iPhone review, the L5 go.

## 3. Slice contents

### 3.1 L1 Backend move

- Copy from finance-app: `Cargo.toml` (renamed `investing-app`, dependencies pruned to what compiles), `src/{main,lib,auth,config,error,health,http,idempotency,py,query}.rs`, `src/research/**`, `src/thesis/**`, `src/macro_watch/**`, `src/sheets/**`, and the stock-portfolio parse from `src/finance/portfolio.rs` moved to `src/portfolio.rs` (only what `GET /api/finance/stock-portfolio` and the research scheduler's `SheetsPortfolio` need).
- Not copied: the rest of `src/finance` (transactions, budget, summary, writes), `src/idx.rs`.
- Tests and fixtures: `tests/research_*.rs`, `tests/macro_watch.rs`, `tests/auth.rs`, `tests/sheets_client.rs`, `tests/common/`, `tests/fixtures/` (research, macro, sheets parts), and the stock-portfolio unit tests. Pack TOMLs and `rules.toml` from `research/`.
- Renames: `FINANCE_*` → `INVESTING_*` env vars (keep `PORTFOLIO_SPREADSHEET_ID`, `GOOGLE_APPLICATION_CREDENTIALS`); cookie `finance_session` → `investing_session`; header `X-Finance-Client` → `X-Investing-Client`; cache header `X-Finance-Cache-Date` → `X-Investing-Cache-Date`; default bind `127.0.0.1:8789`; default data dir `~/investing-app-data`.
- Routes: `health`, `auth`, `/api/research/*`, `/api/thesis/*`, `/api/macro/*`, `GET /api/finance/stock-portfolio`. Nothing else.
- `deploy/`: `run.sh`, `install.sh`, `scraper.sh`, plists `com.menganto.investing(.scraper).plist` as parameterised templates (no home paths, loopback bind), `RUNBOOK.md` carrying the research sections of finance-app's runbook (migrations, `--reingest`, structure drafts, rollback). Not loaded anywhere in L1.

### 3.2 L2 Web shell

Branch `slice/l2-web-shell` from `main`. depends_on: L1 merged (Vitest `components/charts/driverGeometry.test.ts` and the e2e mocks read `tests/fixtures/research/**`; the thesis mocks read `tests/fixtures/thesis_golden/`). Rollback: close the PR. Decisions D1, D7 (prd.md §9) and D8, D13, D15, D16 (design.md, decisions table) apply.

**Copy** finance-app `web/` at the L1 source commit, minus the "Not copied" list. Trial on `4ef2dd9` (2026-10-27, scratch copy with finance-app `tests/fixtures/research/` beside it as L1 leaves it): this tree plus edits 1, 2, 3 (without the new `navState`/`BackHeader`), 4, 6, 7 and 8 below passes `npx tsc -b`, `npm test` (15 files, 216 tests) and `npm run build`; `npm run icons` with the "I" glyph of edit 9 writes the four PNGs.

- Copied: `package.json`, `package-lock.json`, `index.html`, `vite.config.ts`, `playwright.config.ts`, `tailwind.config.ts`, `postcss.config.js`, `tsconfig{,.app,.node,.sw}.json`, `public/`, `scripts/gen-icons.mjs`, `src/{main.tsx,App.tsx,sw.ts}`, `src/{components,lib,pwa,styles,test}/`, `src/assets/fonts/`, `src/screens/PlaceholderScreen.tsx`, `src/shell/{AppShell,AuthGate,UnlockScreen,Banner,OfflineBanner,UpdateBanner,BottomNav}.tsx`, `src/shell/{nav.ts,shell.test.tsx}`, and `e2e/research/{mockApi,audits}.ts` moved to `e2e/`.
- Not copied: `src/catalog/` (dev-only component catalog; it also holds a `finance_session` string), `src/features/` (L3), `src/screens/{home,report,portfolio,thesis}/` (spending; Portofolio and Tesis arrive in L3), `src/shell/{Fab,DesktopSidebar,DetailPanel}.tsx`, `src/shell/AppShell.test.tsx` (tests the desktop `<main>`), `src/components/{AccountCard,Avatar}.tsx`, `src/lib/{finance.ts,finance.test.ts,categoryIcons.ts}`, `src/assets/{banks,category}/`, `e2e/research/*.e2e.ts` (they drive `#/research` routes; §3.3 says which lane re-creates what).
- Kept on purpose: `lib/categories.ts` is the `cat-1…8` chart palette that `DonutChart`, `donutGeometry` and `BudgetBar` read (no spending data). `components/TransactionRow.tsx` holds the row and `ListState` that `DataTable`, Portofolio and the thesis views use.

**Change** (all in L2):

1. `main.tsx`: delete the `/__components` catalog branch (finance-app `main.tsx:9-17`); render `<App />`, register the service worker in production.
2. `App.tsx`: the route table of design.md §2.2, every route rendering `PlaceholderScreen` (pushed routes with `BackHeader`); unknown → `#/beranda`; no `AddTransaction*`, `research`, `riset`, `/idx`, `/macro` routes. This is the one route table; each L3 lane changes only its own lines (§3.3).
3. Shell (D13, D8): `nav.ts` → the 4 tabs of design.md §2.1, no `DESKTOP_ENTRIES`; `BottomNav` → `grid-cols-4`, active = owner tab; `AppShell` → the mobile layout only, `useShell`, `ShellContext`, `DetailContent`, the `isDesktop` branch, `Fab`, `onAdd` and `sidebar` removed; `AuthGate` `CheckingShell` → mobile layout only. New `shell/navState.ts` + `navState.test.ts` and `shell/BackHeader.tsx` per design.md §2.4. Drop the FAB's 26 px from the scroll padding in `styles/index.css:17`.
4. Desktop branches in carried charts (D13): `DonutChart.tsx:55-56` and `TrendChart.tsx:64-65` default `size` to `"mobile"`, no `useIsDesktop`. `lib/useMediaQuery.ts` stays until L3a removes `FactDrawer`'s branch (§3.3).
5. Spending symbols carried screens need: `MONTHS_ID_SHORT` (finance-app `lib/finance.ts:98`) becomes an export of `lib/time.ts`, for `IndicatorTrend.tsx:2` (L3a) and `ThesisScreen.tsx:13` (L3b).
6. Tests: `BudgetBar.test.tsx` loses the `AccountCard` describe; `shell.test.tsx` loses the Riset-entry, `Fab` and `DesktopSidebar` describes, its `BottomNav` case uses `portofolio`/"Portofolio", its unlock heading is "Buka kunci Investasi".
7. Renames, so `git grep -nE 'FINANCE_|finance_session|X-Finance' -- web` is empty: `lib/api.ts:9` `X-Finance-Client` → `X-Investing-Client`; `lib/api.ts:12` and `pwa/swPolicy.ts:13` `X-Finance-Cache-Date` → `X-Investing-Cache-Date`; comments `lib/api.ts:4,6,242` and `sw.ts:6` → `investing_session`, `INVESTING_APP_TOKEN`; header asserts in `api.test.ts`; `api.test.ts:63` absolute-URL case → `https://example.com/api/x`. Naming only (separate origin): cache prefixes `swPolicy.ts:6,9` → `investing-api-`, `investing-shell-` (and `swPolicy.test.ts`); `lib/privacy.tsx:7` → `investing.hideAmounts`; Vite plugin `financeServiceWorker` → `investingServiceWorker`; `name` → `investing-web` in `package.json` and both `name` fields at the top of `package-lock.json` (no dependency change).
8. Dev proxy (D1): `vite.config.ts:49` target → `http://127.0.0.1:8789`. D1 allows that literal in `web/vite.config.ts`; the `check.sh` allowlist is the guardrails lane's.
9. PWA and titles per design.md §4: glyph "I" in `scripts/gen-icons.mjs` (replace `inF`, `:21`, and its call at `:57`), `npm run icons`, commit the four PNGs; `public/favicon.svg` "I" path; `manifest.webmanifest` `name`/`short_name` `Investasi`; `index.html` `<title>` and `apple-mobile-web-app-title`; `UnlockScreen.tsx:69` "Buka kunci Investasi" plus the sandbox line keyed off `GET /api/health` `sandbox` (design.md §3.6, D7).

**e2e** (D15: `vite preview` + `page.route` mocks, no backend):

- `e2e/mockApi.ts` (from `e2e/research/mockApi.ts`) stops importing `src/features/**`: the `MACRO` value of finance-app `features/research/desk/macro.testutil.ts` and `proposalsResponse()` of `features/research/testing/extract.ts` are written once to `e2e/fixtures/macro-observations.json` and `e2e/fixtures/proposals.json`. Added routes: `/api/health` → `{"status":"ok"}`; `/api/thesis/summary`, `/api/thesis/{AAAA,BBBB,CCCC}`, `/api/thesis/macro`, `/api/thesis/drafts/vault` → `tests/fixtures/thesis_golden/{summary,detail_*,macro,drafts_vault}.json`; `/api/thesis/drafts` → `{ drafts: [], vault_drafts: <drafts_vault.vault_drafts>, total }`; `/api/finance/stock-portfolio` → new `e2e/fixtures/stock-portfolio.json`, same shape as the `data` object in finance-app `screens/portfolio/portfolio.test.tsx:8-31` but with tickers COAL and REST, so a position's company page has canon data. All values synthetic; no owner data.
- `playwright.config.ts`: one project, `webkit-iphone15` (`devices["iPhone 15"]`), `testMatch: "**/*.e2e.ts"`; the Desktop Chrome project is dropped (D13; CI installs WebKit only).
- Specs: `e2e/shell.e2e.ts` (bar has 4 links, each ≥ 44 × 44 px, no "Catat transaksi" button, also at 200 % root font: the bottom-nav cases of finance-app `e2e/research/layout.e2e.ts:74-112`); `e2e/floor.e2e.ts` (the automated floor, D16: a `ROUTES` table with every design.md §2.2 route, each checked with `horizontalOverflow`, `smallTargets`, `axeSeriousCritical` from `e2e/audits.ts`, screenshot to `docs/qa/<slice>/<route>.png`); `e2e/nav.e2e.ts` (design.md §2.4 rules 3, 5–9 on the placeholder routes, and the unlock-screen sandbox line with `/api/health` mocked `sandbox: true` and absent).

**iPhone checklist** (D16): `docs/iphone-checklist.md` is the manual real-device list, run by the owner at L4, not an automated gate. Rewrite of finance-app `docs/research/iphone-checklist.md`:

| finance-app row | investing-app |
|---|---|
| Setup `:8443`, "Riset" | Setup `https://<mac>.<tailnet>.ts.net:8445`, install as "Investasi"; build noted by `sw.js` hash |
| M2, M3 (offline, half-up Tailscale) | Same, on Beranda and a company page |
| M4, M5 (Jurnal composer with keyboard; "8 px above the FAB") | Same on the company page Jurnal block; "above the bottom bar" |
| M6 (Desk sheets) | Beranda filter sheet and inbox item sheet |
| M7, M8, M9, M11, M13, M14 | Same checks; Desk → Beranda, Dossier → company page |
| M10 ("Riset" tab reachable at largest text) | All four tabs reachable at largest text and Safari 200 % |
| M12 (VoiceOver; FAB "Catat transaksi") | VoiceOver: four tab names, back control "Kembali ke …", eye toggle state, Sheets announced as dialogs |
| new M15 | Tab memory: scroll Perusahaan, open a company page, switch to Portofolio and back: same page and scroll |
| new M16 | Portofolio position → Sheet numbers → **Buka halaman perusahaan** → "‹ Portofolio" returns to the list |
| new M17 | finance-app (`:8443`) and investing-app (`:8445`) both installed: unlocking one never locks the other |

No widths in the checklist: layout at 393 px is the automated floor's job.

### 3.3 L3 Screens

**Order** (D14, D17). L3a branches from `main` after L2 merges. L3b and L3c branch from `main` after L3a merges and run in parallel. Every PR is a draft into `main`; no stacked bases. Branches `slice/l3a-beranda-alat`, `slice/l3b-perusahaan`, `slice/l3c-portofolio`. L3b depends_on L3a (merged); L3c depends_on L3a (merged). Rollback: close the PR.

**Copy rule.** Start from the entry files of each lane and copy what they import, following relative imports (type-only included), CSS `url()` and `import.meta.glob`, with the exceptions below. Files L2 already carries (`components/`, `lib/`, `shell/`, `pwa/`) are not copied again. Measured on `4ef2dd9` with the exceptions applied: L3a reaches 26 files under `features/`, L3b 73, L3c none (`PortfolioScreen.tsx` alone).

| Lane | Entry files (finance-app `web/src/`) | Builds |
|---|---|---|
| L3a | `features/riset/screens/{Landing,Compare,Calibration,Packs}.tsx`, `features/research/desk/{MacroContextCard.tsx,useMacro.ts,useInbox.ts}` | Beranda (design.md §3.1, new `MacroGrid`), Alat list (new `ToolRow`) and the three tools (§3.5); `Landing.tsx` split into `features/beranda/Beranda.tsx` (Kotak masuk, Kalender, Makro) and `features/perusahaan/CoverageList.tsx` (the "Perusahaan saya" rows and groups, exported, not routed) |
| L3b | `features/riset/CompanyPage.tsx`, `features/riset/screens/Add.tsx`, `screens/thesis/ThesisScreen.tsx` | Perusahaan screen on `CoverageList` + "Tesis lain" + `VaultDraftsSheet` (§3.3), company page + `ThesisBlock` (§3.4) with the `tesis` block id below, Tambah with `?ticker=` prefill |
| L3c | `screens/portfolio/PortfolioScreen.tsx` | Portofolio (§3.2) with `PositionSheet` and the eye toggle |

**`CoverageList` contract** (L3a exports, L3b consumes). File `features/perusahaan/CoverageList.tsx`, built from finance-app `Landing.tsx:25-89` (`CompanyRow`, `Companies`) and `landingData.ts:20-33` (`CoverageGroups`, `groupCoverage`, which stay in `landingData.ts`):

```ts
import type { CoverageRow, DeskResponse } from "../research/types";
export function CompanyRow(props: { row: CoverageRow }): JSX.Element;   // 2-line row, link → #/perusahaan/<T>
export function CoverageList(props: { desk: DeskResponse }): JSX.Element;
```

`CoverageList` renders design.md §3.3 blocks 2–4 only: Dipegang (`held`), Manual (`manual`, omitted when empty), Pembanding (n) (`peers`, lipat, omitted when empty). It does not fetch, and it has no header, **Tambah**, loading, error, "Tesis lain" or footer: L3b's Perusahaan screen calls `useDesk()` and owns those. When `held` and `manual` are both empty it renders finance-app's empty text with a **Tambah emiten** link to `#/perusahaan/tambah`. L3a's `Beranda.tsx` does not use it.

**`tesis` block id** (L3b; `features/riset/sections.ts` is reached by L3b's closure only). finance-app `sections.ts:1,4,6,9-22` has no `tesis`, so `sectionTarget("tesis")` returns `null` (`:25-29`) and `CompanyPage.tsx:46-53` neither opens nor scrolls. L3b adds `"tesis"` to `BlockId` and to `IDS` between `"jurnal"` and `"harga"`, `tesis: false` to `COLLAPSIBLE` (lipat), and `thesis: "tesis"` to `ALIASES`; `CompanyPage.tsx:32` adds `tesis: COLLAPSIBLE.tesis ?? false` to the open state (true in the thesis-only state). `ThesisBlock` renders with `id="tesis"`. Unit test: `sectionTarget("tesis")` and `sectionTarget("thesis")` return `"tesis"`; `#/perusahaan/COAL?section=tesis` opens the block and scrolls it to the top.

**Ownership** (D14). A file reached by L3a's closure, or by both L3a's and L3b's, belongs to L3a; a file only one lane reaches belongs to that lane. Measured shared L3a∩L3b files: `features/research/{api,types,format}.ts`, `features/research/components/{FactChip,FactDrawer,ProvenanceBadge}.tsx`, `features/riset/ui.tsx`. L3a also owns the `App.tsx` route table and the `ROUTES` table of `e2e/floor.e2e.ts`: it lands every design.md §2.2 route, with L3b's and L3c's still on `PlaceholderScreen`; each later lane changes only its own lines (a route-line conflict on rebase is resolved by keeping both lines). L3b and L3c share no file except those two tables. `ThesisBlock` and `VaultDraftsSheet` live outside `features/research/` (e.g. `features/thesis/`), so `features/research/independence.test.ts` keeps forbidding `/api/thesis` there.

**Closure exceptions** (move the symbol into a carried file, rewrite the import, do not carry the source file):

| Symbol | From (finance-app) | To | Importers to repoint | Lane |
|---|---|---|---|---|
| `DeskFooter`, `ScreenFooter` (+ `range` helper) | `features/research/desk/DeskScreen.tsx:16-48` | new `features/research/desk/DeskFooter.tsx` (with `DeskFooter.test.tsx`) | `Landing.tsx:12`, `Compare.tsx:9`, `Calibration.tsx:4`, `Packs.tsx:7` | L3a |
| `TYPE_WORD` | `desk/MobileInbox.tsx:25` | `desk/useInbox.ts` | `Landing.tsx:13` | L3a |
| `SourcedStat` | `research/dossier/mobile/SourcedStat.tsx` | `research/components/SourcedStat.tsx` | `riset/blocks/{AngkaKunci,Harga}.tsx` | L3b |
| `useSheetTrigger` | `research/dossier/mobile/useSheetTrigger.ts` | `research/components/useSheetTrigger.ts` | `riset/StructureMindMap.tsx`, `riset/blocks/{KuartalTerakhir,Struktur,Klaim}.tsx` | L3b |
| `PackOverride` (+ `AUTO`) | `research/dossier/DossierHeader.tsx:15-121` | `research/components/PackOverride.tsx` | `riset/blocks/Header.tsx:3` | L3b |
| `Financials` | `research/dossier/sections/Financials.tsx` | `research/components/Financials.tsx`; its `SectionProps` (`sections/index.ts:16-20`) into `research/types.ts` | `riset/blocks/Keuangan.tsx:1`; `research/components/ClaimCard.tsx:10` imports `DossierResponse` from `types.ts` instead of `DossierPayload` | L3b |
| `DetailStat` | `screens/home/TxParts.tsx:84-86` | new `components/DetailStat.tsx` | `PortfolioScreen.tsx:22` | L3c |
| `InvestTabs` | `screens/portfolio/PortfolioScreen.tsx:51` | deleted with the segmented control | `ThesisScreen.tsx:16` (L3b), `PortfolioScreen.tsx:121` (L3c) | L3b, L3c |
| `MONTHS_ID_SHORT` | `lib/finance.ts:98` | `lib/time.ts` (L2) | `desk/IndicatorTrend.tsx:2` (L3a), `ThesisScreen.tsx:13` (L3b) | — |

With these moves, L3b's closure reaches no file under `dossier/` and none of `DeskScreen`, `MobileDesk`, `MobileInbox`, `MobileCoverage`, `CoverageBoard`, `desk/Inbox`, `desk/Calendar`, `TxParts`, `research/routes.tsx` (measured). Never carried: those files, `dossier/*`, `screens/home/*`.

**Desktop removal** (D13), each in the lane that owns the file: `PortfolioDesktop` and its `useShell`/`openDetail` (`PortfolioScreen.tsx:21,89-93,274-345`) and the `#/portfolio/:ticker` screen (L3c); `ThesisDesktop` and its `useShell` (`ThesisScreen.tsx:15,113-116,170-230`) (L3b); `FactDrawer`'s `useIsDesktop` branch (`FactDrawer.tsx:6,248`), then `useIsDesktop` itself if unused, and `PAGE`'s `max-w-[720px]` (`riset/ui.tsx:103`) (L3a).

**Old routes.** Each lane rewrites the `#/riset`, `#/research`, `#/portfolio`, `#/thesis`, `#/idx` literals in the files it owns to design.md §2.2 routes, adding `from=` per §2.4 rule 4; `riset/ui.tsx:88` `BackLink to="/riset"` gives way to `BackHeader` (L3a). Files with old literals at `4ef2dd9` (non-test, carried): `riset/screens/{Landing,Compare,Calibration}.tsx`, `riset/ui.tsx` (L3a); `riset/CompanyPage.tsx`, `riset/blocks/Header.tsx`, `riset/screens/Add.tsx`, `research/add/StepTicker.tsx`, `research/components/MetricRow.tsx` (`/research/<T>?section=paket`), `screens/thesis/ThesisScreen.tsx` (L3b); `screens/portfolio/PortfolioScreen.tsx` (L3c). After L3c this prints nothing (comment lines are filtered; relative imports and `/api/…` paths do not match):

```
git grep -nE '(^|[^i.])/(riset|research|portfolio|thesis|idx)([/?"`$]|$)' -- web/src ':!*.test.*' ':!*.testutil.*' | grep -vE ':[0-9]+:[[:space:]]*(\*|//|/\*)'
```

On finance-app `4ef2dd9` the same command lists the literals above plus files that are not carried.

**Tests.** Carried with their files when their imports resolve. Ported, not dropped:
- `features/research/{vocabulary,provenance,display}.test.tsx` and `independence.test.ts` (no verdict words, provenance, display rules, no foreign API prefix) render `canonSurfaces.testutil.tsx` `SURFACES`, which mounts `ResearchRoutes` on `/research/*`. L3a rewrites `SURFACES` to mount `App`'s route table on the design.md §2.2 routes it lands (Beranda, the three tools); L3b adds the company page sections, Perusahaan and Tambah; L3c adds Portofolio.
- `features/riset/companyPage.test.tsx`, `features/riset/screens/screens.test.tsx`, `features/riset/leftovers.test.tsx` import `dossier/*`: repoint to the moved modules; cases that assert `#/riset` routes or desktop layout are rewritten for the new routes.
- `screens/portfolio/portfolio.test.tsx` (L3c) and `screens/thesis/thesis.test.tsx` (L3b): rewritten for the Sheet and the Tesis block.

Dropped, each listed by path in the lane's PR body with one line why: `features/research/routes.test.tsx` (tests `ResearchRoutes`, `IdxTickerRedirect`, finance-app `App.tsx` and `nav.ts`), every test under `features/research/dossier/`, `desk/{DeskScreen,MobileDesk}.test.tsx`, and any other case that only covers a desktop variant.

**e2e per lane** (in `webkit-iphone15`, mocks from §3.2): L3a `beranda.e2e.ts` (finance-app `macro.e2e.ts` cases on the Makro grid, `overlays.e2e.ts` inbox sheets), L3b `perusahaan.e2e.ts` (company page sections incl. Struktur, Keuangan and Harga on COAL and REST, the claim sheet and `FactDrawer` from `overlays.e2e.ts`, the long-name and 1e15 cases of `layout.e2e.ts`, Tesis block, Draf vault, `?ticker=` prefill), L3c `portofolio.e2e.ts` (position Sheet numbers, eye toggle, **Buka halaman perusahaan** → "‹ Portofolio"). L3c also adds the prd.md S4 reach test to `nav.e2e.ts`: every place one tap from the bar; a company page within 2 taps of Perusahaan, Portofolio and a Beranda inbox item.

### 3.4 L4 Side by side

1. `cp -c -R ~/finance-app-data ~/investing-app-data` (APFS clone). Copy the thesis drafts file into it.
2. Write `~/.hermes/secrets/investing-app.env` (new `INVESTING_APP_TOKEN`, sources unchanged); the agent never prints the token, the owner gets it via `pbcopy`.
3. Owner runs `./deploy/install.sh` in their own terminal (the Hermes guard refuses `launchctl bootstrap`); `tailscale serve --bg --https=8445 http://127.0.0.1:8789`.
4. `scripts/parity.sh` (prd.md A1) and `scripts/counts.sh` (A2) against `:8787` and `:8789` on the same snapshot; the sandbox scheduler stays off (`INVESTING_RESEARCH_DISABLE_SCHEDULER=1`) so the clone does not drift.
5. iPhone: install from `:8445`, run the iPhone checklist on every route (design.md §2.2) with a real held ticker, owner uses it alongside finance-app.

### 3.5 L5 Cutover (after the owner's go)

1. Back up `~/finance-app-data` (`sqlite3 .backup` for `research.sqlite`, copy the rest).
2. finance-app: set `FINANCE_RESEARCH_DISABLE_SCHEDULER=1`, restart; owner stops editing research there.
3. Replace `~/investing-app-data` with a fresh clone; `counts.sh` again (0 differences); enable the investing-app scheduler; restart.
4. Move the scraper job: owner boots out `com.menganto.finance.scraper`, installs `com.menganto.investing.scraper`.
5. Health 200 for `/api/health` and `/` on `:8789` and `:8445`; one-hour watch of logs and the scheduler's first run.
6. Rollback until L6 merges: re-enable finance-app's scheduler, use `:8443` again.

### 3.6 L6 Removal from finance-app

A finance-app PR (its repo, its canon): delete `src/research`, `src/thesis`, `src/macro_watch`, the stock-portfolio route, their tests and fixtures, `features/research`, `features/riset`, `screens/{portfolio,thesis}`, the Investasi and Riset tabs and `#/riset`, `#/research`, `#/idx` routes, research env vars and the scraper plist; update finance-app's `prd.md`, `design.md`, `plan.md` and `RUNBOOK.md`. finance-app's new navigation is decided in that PR, not here.

## 4. Checks per slice

| Slice | Commands (run from a fresh clone of the PR branch) |
|---|---|
| L1 | `cargo fmt --check` · `cargo clippy --all-targets -- -D warnings` · `cargo test --no-fail-fast` (canon goldens included) · start the release binary on an APFS clone of `~/finance-app-data` with a throwaway token on a spare port → `curl -sf /api/health` 200 and one authenticated `GET /api/research/desk` 200 · `git grep -nE 'FINANCE_|finance_session|X-Finance' -- ':!*.md'` empty |
| L2 | `scripts/check.sh` OK (its `web` part is `npm ci && npm test && npm run build`; `build` runs `tsc -b`) · in `web/`: `npx playwright install webkit && npm run e2e` (project `webkit-iphone15`, mocks only, D15): `shell.e2e.ts` 4 links each ≥ 44 × 44 px, no FAB, also at 200 % root font; `floor.e2e.ts` every design.md §2.2 route (placeholders): no page overflow, targets ≥ 44 px, axe no serious/critical; `nav.e2e.ts` design.md §2.4 · screenshots committed under `docs/qa/l2/` and linked in the PR body (D17) · `docs/iphone-checklist.md` rewritten (not run) |
| L3 | L2 checks plus: `floor.e2e.ts` on the lane's real screens (mock data: canon goldens, thesis goldens, `e2e/fixtures/stock-portfolio.json`; no backend, no live data); the lane's spec (§3.3 "e2e per lane"); the ported guard tests pass in `npm test`; tests dropped are listed in the PR body; screenshots under `docs/qa/l3a/`, `docs/qa/l3b/`, `docs/qa/l3c/`. After L3c: the old-route `git grep` of §3.3 is empty. Real held tickers (BUMI, BRIS) are checked at L4, not here |
| L4 | `scripts/parity.sh` exit 0 · `scripts/counts.sh` exit 0 · `docs/iphone-checklist.md` passes at 393, 768, 1280 |
| L5 | Health and `/` 200 on `:8789` and `:8445` · `counts.sh` exit 0 after the final clone · scheduler first run logged without errors |
| L6 | finance-app full gates (fmt, clippy, cargo test, tsc, vitest, build, e2e) · spending acceptance from finance-app `prd.md` §3.1 · `git grep -nE 'api/(research|thesis|macro)|stock-portfolio'` in finance-app empty outside docs |

## 5. Rollback

| Stage | How to go back | Data at risk |
|---|---|---|
| L1–L3 | Close the PR | None; nothing runs |
| L4 | Stop `com.menganto.investing`, `tailscale serve --https=8445 off` | None; sandbox data is a clone |
| L5 | Re-enable finance-app's scheduler, restore the finance scraper job, use `:8443` | Edits made in investing-app after cutover; export them from `research-log.jsonl` if needed |
| L6 | Revert the finance-app removal PR | None; finance-app data kept 30 days (prd.md O4) |
