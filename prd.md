# prd.md — Investing App

Status: draft v0 for owner review · Owner: Menganto · Date: 2026-10-27
Related: [`design.md`](design.md) (screens and navigation), [`plan.md`](plan.md) (build and cutover order).
Source of everything moved: `fauzibrevy-hub/finance-app` at `origin/main` `4ef2dd9` (2026-10-27). Its research canon (`docs/research/prd.md`, `design.md`, `redesign-brief.md`, `ui-slicing.md`) stays the reference for *what each screen shows*; this document says what moves, where it goes, and when the move is done.

This document says *what* must be true and *why*. How it is built is in `plan.md`. Where something is unknown it says `TBD` and names the way to find out.

---

## 1. Problem

Investing lives inside finance-app as two of its four phone tabs (`web/src/shell/nav.ts`: Beranda, Laporan, Investasi, Riset, plus the centre FAB for recording a transaction). The Riset tab opens one long landing that stacks the owner's companies, the inbox, the calendar and a "Lainnya" link list; every company page and tool hangs one level below it and every trip goes back through it. Giving companies, inbox, calendar and tools their own places is not possible inside one tab without pushing spending tabs out of the bar.

The two halves also no longer have the same shape or pace. Measured at `4ef2dd9`:

| Part | Rust lines | Web lines |
|---|---|---|
| Investing: `src/research`, `src/thesis`, `src/macro_watch`, `src/finance/portfolio.rs` | ~33,300 | ~30,600 (`features/research`, `features/riset`) plus Portfolio and Thesis screens |
| Spending: rest of `src/finance`, `src/sheets` | ~2,800 | Beranda, Laporan, FAB sheet |

Every research release (store migrations, `--reingest`, pack changes) is a deploy of the app the owner uses several times a day to record spending, and the spending side carries ~30k lines of research code it never calls.

**Owner decision (2026-10-27):** investing leaves finance-app for its own app and repo. finance-app keeps only spending.

## 2. Goal

One installed investing app on the iPhone, separate from finance-app, that holds everything investing (Riset, Portfolio, Thesis, Macro) behind a four-tab bottom navigation:

| Tab | Holds |
|---|---|
| **Beranda** | What needs the owner now: inbox, filing calendar, macro indicators |
| **Portofolio** | Holdings from the portfolio Sheet: value, P/L, composition, allocation vs target |
| **Perusahaan** | Company list → company page (stage 1 layout), with the owner's thesis inside the company page |
| **Alat** | Bandingkan, Kalibrasi, Paket & aturan |

v1 is a **move, not a rewrite**: every number and every write the owner can do in finance-app's investing screens today works the same in the new app, under the new navigation.

## 3. User

- **Only user:** the owner, Menganto. Long-term investor in a small set of IDX equities (held at `4ef2dd9`: BRIS, BUMI, LEAD, SOCI; plus manual tickers and peers).
- **Primary device:** iPhone 15 (393 × 852), installed as a Home Screen PWA over Tailscale. Reading **and** editing happen on the phone (redesign-brief Q2).
- **Secondary:** Mac browser, same one-column layout centred at ≤ 720 px. No separate desktop layout.
- **Not for:** anyone else, intraday trading, whole-exchange screening, buy/sell instructions.

## 4. Scope

### 4.1 v1 (the move)

1. **Riset, as it is on finance-app `main`:** fact store and XBRL/PDF ingest, standalone/TTM/annual views, industry packs, KPI entry and document extraction proposals, prices and drivers, claims, journal, calibration, inbox and calendar, structure map with draft import, add-ticker wizard, packs and rules, scheduler. Endpoints `/api/research/*` unchanged.
2. **Portfolio:** read-only holdings from the portfolio Sheet (`PORTOFOLIO!A1:AE35`), same parse as today. Endpoint `GET /api/finance/stock-portfolio` kept at the same path in v1 so parity is a plain JSON diff.
3. **Thesis:** read the owner's thesis dossiers from the vault (read-only), list/create/delete thesis drafts. Endpoints `/api/thesis/*` unchanged. The app never writes to the vault.
4. **Macro:** indicator ingest, observations and stream as today. Endpoints `/api/macro/*` unchanged. The Beranda macro card shows indicators only (no score, regime or lens on screen; finance-app decision R7.4, 2026-10-25).
5. **New navigation** per §2 and `design.md` §2–§3.
6. **Own runtime:** own repo, process, port, data directory, token, launchd service, Tailscale port, PWA name and icon (§6).
7. **Side-by-side period, then cutover** (§7), then removal of investing from finance-app in its own PR.

### 4.2 Out of scope

- New research features, the stage 2 driver model, and redesign of company-page blocks. Company-page content is the stage 1 layout from finance-app PR #51 and later fixes, unchanged.
- Spending features, the expense-tracker bot (`~/expense-tracker`), and finance-app's own navigation after removal (that PR updates finance-app's canon, not this repo).
- Legacy `/api/idx/*` (Data IDX, superseded 2026-10-24); not moved.
- Any model inside the app. LLM drafting (structure maps) stays outside the app, by Claude Code only, imported as proposals.
- Buy/sell vocabulary, system verdicts, composite scores, system fair value.
- Changes to Google Sheets structure, the vault, or the IDX scraper's code.
- Cloud hosting, multi-user, push notifications.
- Importing finance-app's git history (clean copy; `plan.md` §1).

## 5. Success metrics

| # | Metric | Target | How measured |
|---|---|---|---|
| S1 | API parity | 0 differences on every v1 GET (§8 A1) between finance-app and investing-app on the same data | `scripts/parity.sh`, `jq -S` with request-time stamps dropped |
| S2 | Record counts | Identical per-ticker counts of filings, facts, KPI cells, claims, evaluations, journal entries, documents, proposals, structure nodes; identical thesis-draft and macro-observation counts | `scripts/counts.sh` on both data directories |
| S3 | Phone floor | 0 failures of `iphone-checklist` at 393 px on every route: no page sideways scroll, no text collision or cut digits, targets ≥ 44 px | Playwright WebKit, iPhone 15 descriptor |
| S4 | Reach | Every place in §2 is one tap from the bottom bar; any company page is ≤ 2 taps from Perusahaan, Portofolio or a Beranda inbox item | e2e route test |
| S5 | Owner acceptance | Owner signs off on the iPhone after using the app alongside finance-app | Owner's OK recorded in the cutover PR |
| S6 | Independence | finance-app after removal has no `/api/research`, `/api/thesis`, `/api/macro`, `stock-portfolio` routes or screens, starts without any research env var, and its spending acceptance still passes | finance-app gates in the removal PR |

## 6. Runtime and data

| Item | finance-app today | investing-app |
|---|---|---|
| Repo | `fauzibrevy-hub/finance-app` | `fauzibrevy-hub/investing-app` (private), local `~/investing-app` |
| Bind | `127.0.0.1:8787` | `127.0.0.1:8789` (free on 2026-10-27; re-check at install) |
| Tailnet | `https://<mac>.<tailnet>.ts.net:8443` | `:8445` (443 = Mission Control, 8443 = finance-app, 8444 = finance-app sandbox, 9119 = Hermes dashboard) |
| Data dir | `~/finance-app-data` | `~/investing-app-data` |
| Env file | `~/.hermes/secrets/finance-app.env` | `~/.hermes/secrets/investing-app.env` |
| Token | `FINANCE_APP_TOKEN` | `INVESTING_APP_TOKEN` (new, ≥ 32 chars; never shared with finance-app) |
| Env prefix | `FINANCE_*` | `INVESTING_*` (same meanings; `PORTFOLIO_SPREADSHEET_ID` and `GOOGLE_APPLICATION_CREDENTIALS` keep their names) |
| Session cookie | `finance_session` | `investing_session` (cookies are not port-isolated on one host, so the names must differ) |
| Service | launchd `com.menganto.finance` | `com.menganto.investing` |
| Scraper job | `com.menganto.finance.scraper` | `com.menganto.investing.scraper` after cutover (the scraper feeds research only) |

Data that moves into `~/investing-app-data` (copied, never moved, until §7 step 4):

- `research.sqlite` (+ WAL), `research-log.jsonl`, `research-idempotency.jsonl`, `research/` (structure drafts and other research files), `macro_store.json`.
- The thesis drafts file named by finance-app's `FINANCE_THESIS_DRAFTS_PATH`, copied as is.
- Read in place, not copied: the vault (`*_VAULT_ROOT`, read-only), the IDX scraper's XBRL directory and PDF filings directory, the portfolio Sheet.

Data that stays with finance-app: `idempotency.jsonl` (transaction writes) and everything spending.

Auth model is unchanged from finance-app (`prd.md` §5 there): one token, Bearer for scripts, unlock screen → HMAC session cookie for the PWA, `X-Finance-Client`-style CSRF header on cookie writes (renamed `X-Investing-Client`), `/api/health` the only open route.

## 7. Cutover rule (owner decision 2026-10-27)

1. **Side by side.** investing-app runs on its own port with a fresh APFS clone of `~/finance-app-data`. finance-app stays the live writer; nothing the owner does in investing-app during this period is carried over.
2. **Counts match.** `scripts/parity.sh` and `scripts/counts.sh` both report 0 differences against finance-app on the same snapshot.
3. **Owner signs off on the iPhone.**
4. **Switch.** Back up `~/finance-app-data`; stop research writes in finance-app (`FINANCE_RESEARCH_DISABLE_SCHEDULER=1`, no owner edits); take the final clone into `~/investing-app-data`; re-run counts (must be 0 differences); move the scraper job; investing-app becomes the only writer.
5. **Remove** investing from finance-app in its own PR (S6). Until that PR merges, rolling back is: point the owner back at `:8443`, re-enable finance-app's scheduler.

## 8. Acceptance

**A1 API parity.** For each held ticker and one peer, `scripts/parity.sh` diffs these GETs between the two apps and reports 0 differences:
`/api/research/desk`, `/api/research/compare`, `/api/research/calibration`, `/api/research/packs`, `/api/research/rules`, `/api/research/filings`, `/api/research/{T}`, `/api/research/{T}/financials?view=standalone|ttm|annual`, `/api/research/{T}/pack`, `/api/research/{T}/valuation`, `/api/research/{T}/prices`, `/api/research/{T}/structure`, `/api/research/{T}/claims`, `/api/research/{T}/journal`, `/api/research/{T}/proposals`, `/api/finance/stock-portfolio`, `/api/thesis/summary`, `/api/thesis/{T}`, `/api/thesis/drafts`, `/api/macro/observations`. Request-time fields (`as_of` and similar) are dropped by a named jq filter, each with a reason in the script.

**A2 Counts.** `scripts/counts.sh` per S2 reports identical numbers on both data directories, before and after the final clone.

**A3 Writes work in the new app** (sandbox data only): KPI cell PUT and DELETE, document upload and proposal accept/reject, claim create/patch, journal add and annotate, event dismiss, structure draft import and node accept/reject, universe add/override, packs reload, research refresh, thesis draft create and delete, macro trigger. Each returns the same status as finance-app for the same request.

**A4 Navigation.** Four bottom tabs, no FAB; each tab keeps its own scroll and last sub-route; every pushed screen has an explicit back control ≥ 44 px; deep links `#/perusahaan/:T?section=<block>` scroll to the block; old finance-app hash routes are not supported (no redirects).

**A5 Phone floor.** S3 on every route in `design.md` §2, with the structure map, Keuangan tables and Harga charts checked on a real held ticker.

**A6 Owner sign-off.** S5.

**A7 Removal.** S6, in a finance-app PR merged by the owner after A1–A6.

## 9. Open questions

- [ ] **O1 Thesis without research.** A thesis dossier or draft can exist for a ticker that is not in the research universe. Default: Perusahaan lists them in a collapsed group "Tesis lain (n)"; tapping opens the company page with only the header and the Tesis block, plus **Tambah ke riset**. Confirm at L3 review.
- [ ] **O2 PWA name and icon.** Default: name and short name "Investasi", glyph "I" on the shared accent, so the two Home Screen icons differ by letter and label. Confirm at L2 review.
- [ ] **O3 Unused backend routes.** `/api/thesis/macro`, `/api/macro/scoring`, `/api/macro/registry` have no screen today. Default: moved unchanged (copy, not prune); pruning is a later, separate decision.
- [ ] **O4 Keeping finance-app research data.** After cutover, how long the research files stay in `~/finance-app-data` before the owner deletes them. Default: 30 days, read-only.

## 10. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Owner edits research in finance-app during side by side and expects it in the new app | Medium | Medium: edits lost at cutover | §7 step 1 says it plainly on the unlock screen of the sandbox build; final clone is taken after research writes stop |
| Two schedulers ingest at once after cutover and diverge | Low | High | Step 4 disables finance-app's scheduler before the final clone; removal PR deletes it |
| Copy misses a file the build or runtime needs (untracked asset, pack TOML, fixture) | Medium | Medium | Copy by import closure from a clean checkout; L1/L2 done only when a fresh clone builds and serves |
| Cookie or service-worker cross-talk between the two apps on one host | Medium | Medium: unlock loops, stale shell | Different cookie name and CSRF header; different port = different origin for the service worker and storage; tested on the iPhone at L4 |
| Parity diff stays red because of request-time fields | High | Low | Named jq drops with reasons; anything else that differs is a bug |
| Scope creep into redesign while moving | Medium | Medium | §4.2; screen content changes go to a later brief |
