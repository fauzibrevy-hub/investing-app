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

- Copy `web/` toolchain (`package.json` renamed `investing-web`, Vite, Tailwind config and tokens, `tsconfig`, Vitest and Playwright config), `web/src/{main.tsx,App.tsx,lib,styles,pwa,sw.ts}`, and from `shell/`: `AppShell`, `AuthGate`, `UnlockScreen`, `Banner`, `OfflineBanner`, `UpdateBanner`, `BottomNav`, `nav.ts`.
- Not copied: `Fab`, `DesktopSidebar`, `DetailPanel`, spending screens.
- Copy finance-app `docs/research/iphone-checklist.md` to `docs/iphone-checklist.md`; it is the phone floor for every later slice.
- New nav per design.md §2: 4 tabs, no FAB, per-tab sub-route and scroll memory, `BackHeader` origin label; routes render placeholders until L3.
- Manifest, icons and titles per design.md §4; client header and cookie names per §3.1.

### 3.3 L3 Screens

Copy by import closure: start from each screen file named below and copy exactly what it imports (components, hooks, `api.ts`, `types.ts`, `format.ts`, tests). Desktop-era code under `features/research/` that no carried screen imports stays behind.

- **L3a** Beranda from `features/riset/screens/Landing.tsx` (inbox and calendar blocks) and `features/research/desk/{MacroContextCard,useMacro,useInbox}`; Alat list (new `ToolRow`) and the three tools from `features/riset/screens/{Compare,Calibration,Packs}.tsx`.
- **L3b** Perusahaan from `Landing.tsx` block "Perusahaan saya" + "Tesis lain" group; company page from `features/riset/CompanyPage.tsx` and `blocks/`; new `ThesisBlock` built from `screens/thesis/ThesisScreen.tsx` (dossier render, draft list, draft editor, delete); Tambah from `features/riset/screens/Add.tsx`.
- **L3c** Portofolio from `screens/portfolio/PortfolioScreen.tsx`, minus the segmented control, positions linking to `#/perusahaan/:T`.

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
| L2 | `npm ci` · `npx tsc -b` · `npm test` · `npm run build` · Playwright WebKit iPhone 15: bar has 4 items, each ≥ 44 px, no FAB, no page sideways scroll on any placeholder route |
| L3 | L2 checks plus: e2e for every route in design.md §2.2 at 393 px (no page overflow, targets ≥ 44 px, axe no serious/critical); for the company page, the structure map, Keuangan and Harga on BUMI and BRIS from a sandbox clone; screenshots attached to the PR |
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
