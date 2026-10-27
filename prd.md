# prd.md — Investing App

Status: draft v1 for owner review (QA fixes, decisions D1–D20 of 2026-10-27) · Owner: Menganto · Date: 2026-10-27
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
- **Secondary:** Mac browser, same one-column layout, finance-app's 480 px column centred (D13). No separate desktop layout.
- **Not for:** anyone else, intraday trading, whole-exchange screening, buy/sell instructions.

## 4. Scope

### 4.1 v1 (the move)

1. **Riset, as it is on finance-app `main`:** fact store and XBRL/PDF ingest, standalone/TTM/annual views, industry packs, KPI entry and document extraction proposals, prices and drivers, claims, journal, calibration, inbox and calendar, structure map with draft import, add-ticker wizard, packs and rules, scheduler. Endpoints `/api/research/*` unchanged.
2. **Portfolio:** read-only holdings from the portfolio Sheet (`PORTOFOLIO!A1:AE35`), same parse as today. Endpoint `GET /api/finance/stock-portfolio` kept at the same path in v1 so parity is a plain JSON diff.
3. **Thesis:** read the owner's thesis dossiers from the vault (read-only), list/create/delete thesis drafts. Endpoints `/api/thesis/*` unchanged. The app never writes to the vault.
4. **Macro:** indicator ingest, observations and stream as today. Endpoints `/api/macro/*` unchanged. The Beranda macro card reads `GET /api/macro/observations` only and shows indicators only (no score, regime, lens or live stream; finance-app decision R7.4, 2026-10-25; D12).
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
| S1 | API parity | 0 differences on every GET listed in §8 A1, between a second finance-app instance (release build of `4ef2dd9`) and investing-app, each on its own copy of one snapshot (D2). Live `:8787` is never a parity target | `scripts/parity.sh`, `jq -S` with the named drops in §8 A1 |
| S2 | Record counts | Identical counts per ticker of filings, facts, KPI cells, claims, claim evaluations, journal entries and annotations, documents, evidence links, extraction proposals, structure nodes (`sum(json_array_length(doc))` of `structure_maps`), manual shares, scenarios, manual calendar items, events and dismissed events; identical universe rows (ticker, role) and `seen` rows; identical line counts of `research-log.jsonl` and `research-idempotency.jsonl`, thesis-draft count, macro indicator and observation counts, and file counts of `research/documents/`, `research/structure/`, `research/pdf-filings/` | `scripts/counts.sh <dir-a> <dir-b>` (plan.md §3.1b) |
| S3 | Phone floor | Automated: Playwright WebKit, iPhone 15 descriptor at 393 px, on every route in design.md §2.2: no page sideways scroll, targets ≥ 44 px, axe no serious or critical violations. Manual: `docs/iphone-checklist.md` (real-device list, rewritten in L2 for the new nav) passes on the owner's iPhone at L4 (D16) | Playwright `webkit-iphone15` project in CI; owner's checklist run recorded in the L4 PR |
| S4 | Reach | Every place in §2 is one tap from the bottom bar; any company page is ≤ 2 taps from Perusahaan, Portofolio or a Beranda inbox item | e2e route test |
| S5 | Owner acceptance | Owner signs off on the iPhone after using the app alongside finance-app | Owner's OK recorded in the cutover PR |
| S6 | Independence | finance-app after removal has no `/api/research`, `/api/thesis`, `/api/macro`, `stock-portfolio` routes or screens, starts with none of the variables listed in plan.md §3.6 set, and passes the spending checks that need neither Mission Control nor a live Sheet write (plan.md §3.6) | finance-app gates in the removal PR |

## 6. Runtime and data

| Item | finance-app today | investing-app |
|---|---|---|
| Repo | `fauzibrevy-hub/finance-app` | `fauzibrevy-hub/investing-app` (private), local `~/investing-app` |
| Bind | `127.0.0.1:8787` | `127.0.0.1:8789` (free on 2026-10-27; re-check at install) |
| Tailnet | `https://<mac>.<tailnet>.ts.net:8443` | `:8445` (443 = Mission Control, 8443 = finance-app, 8444 = finance-app sandbox, 9119 = Hermes dashboard) |
| Data dir | `~/finance-app-data` | `~/investing-app-data` |
| Env file | `~/.hermes/secrets/finance-app.env` | `~/.hermes/secrets/investing-app.env` (written by the owner; install scripts never create it) |
| Token | `FINANCE_APP_TOKEN` | `INVESTING_APP_TOKEN` (new, ≥ 32 chars; never shared with finance-app) |
| Env prefix | `FINANCE_*` | `INVESTING_*` (same meanings; `PORTFOLIO_SPREADSHEET_ID`, `GOOGLE_APPLICATION_CREDENTIALS` and `MACRO_DISABLE_FRED` keep their names; `FINANCE_SPREADSHEET_ID` is dropped, spending only; D7) |
| Session cookie | `finance_session` | `investing_session` (cookies are not port-isolated on one host, so the names must differ) |
| Service | launchd `com.menganto.finance` | `com.menganto.investing` (`deploy/install.sh` installs only this; D5) |
| Scraper job | `com.menganto.finance.scraper` | `com.menganto.investing.scraper` (`deploy/install-scraper.sh`), installed at cutover only (§7 step 4); the scraper feeds research only |
| Sandbox flags | — | From L1 smoke to L5: `INVESTING_RESEARCH_DISABLE_SCHEDULER=1`, `INVESTING_MACRO_WATCHER=0`, `INVESTING_SANDBOX=1` in the env file before the first start; L5 removes all three (D4, D7) |

Defaults `127.0.0.1:8789` and `~/investing-app-data` appear in code only in `src/config.rs` (and its tests), `deploy/`, `scripts/{parity,counts,writes}.sh` and `web/vite.config.ts`; finance-app's live values only in `scripts/`; no `/Users/<name>` path outside `*.md` (D1, enforced by `scripts/check.sh` hygiene).

Data that moves into `~/investing-app-data` (copied, never moved, until §7 step 4), and how (D3):

- `research.sqlite` through `sqlite3 <src>/research.sqlite ".backup '<dst>/research.sqlite'"`, never a file copy of `research.sqlite`, `-wal` or `-shm` from a directory a service is writing (a clone taken during writes is not a snapshot: QA l4l6-cutover #3).
- Copied as files: `research-log.jsonl`, `research-idempotency.jsonl`, `macro_store.json`, the thesis drafts file (finance-app's `FINANCE_THESIS_DRAFTS_PATH`, default `<data dir>/thesis_drafts.json`, written to `<dst>/thesis_drafts.json`), and `research/` whole: `watchlist.csv`, `scraper-status.json`, `documents/` (uploads), `structure/` (drafts), `pdf-filings/` (original-filing JSON; the default PDF filings directory is `<data dir>/research/pdf-filings`, finance-app `src/config.rs:151-154`, so it is copied, not read in place). `research/scraper.lock/` is not copied.
- Read in place, not copied: the vault (`*_VAULT_ROOT`, read-only), the IDX scraper's checkout (XBRL directory, `idx.sqlite`, `emitens.csv`), the portfolio Sheet.
- No `INVESTING_*` variable may name a path under `~/finance-app-data` (D4); with the path variables unset every research file resolves under `~/investing-app-data`.

Data that stays with finance-app: `idempotency.jsonl` (transaction writes) and everything spending. These are never copied.

Auth model is unchanged from finance-app (`prd.md` §5 there): one token, Bearer for scripts, unlock screen → HMAC session cookie for the PWA, `X-Finance-Client`-style CSRF header on cookie writes (renamed `X-Investing-Client`), `/api/health` the only open route. `/api/health` also returns `"sandbox": true` while `INVESTING_SANDBOX=1` (D7); the unlock screen's sandbox line keys off it (design.md §3.6).

## 7. Cutover rule (owner decision 2026-10-27; steps in plan.md §3.4–§3.5)

1. **Side by side.** investing-app runs on `:8789`/`:8445` from a snapshot of finance-app's research files (§6, D3), with the scheduler, the macro watcher and the sandbox line set as in §6 before its first start. finance-app stays the live writer; nothing the owner does in investing-app during this period is carried over, and the unlock screen says so.
2. **Counts match.** `scripts/parity.sh` and `scripts/counts.sh` report 0 differences between investing-app and a second finance-app instance (release build of `4ef2dd9`, spare port, throwaway token, scheduler and watcher off) on a second copy of the same snapshot (D2); `scripts/writes.sh` reports the same status for every A3 write.
3. **Owner signs off on the iPhone.**
4. **Switch.** Owner stops both services and finance-app's scraper job; back up finance-app's research files; take the final snapshot into a fresh `~/investing-app-data`; `counts.sh` again (0 differences); investing-app starts with scheduler and watcher on and the sandbox flag gone; finance-app starts with `FINANCE_RESEARCH_DISABLE_SCHEDULER=1` and `FINANCE_MACRO_WATCHER=0`; finance-app's scraper plist leaves `~/Library/LaunchAgents`; `deploy/install-scraper.sh` installs `com.menganto.investing.scraper`. investing-app is then the only research writer.
5. **Remove** investing from finance-app in its own PR (S6). Until that PR merges, finance-app's research files stay writable and untouched for 30 days (D18), and rollback is plan.md §3.5 "Rollback": stop investing-app and its scraper, reinstall finance-app's scraper, replay investing-app's `research-log.jsonl` over a moved-aside finance-app `research.sqlite`, copy documents, structure drafts and thesis drafts back, turn finance-app's scheduler and watcher back on, use `:8443`.

## 8. Acceptance

**A1 API parity.** For each held ticker `{T}` and one peer, `scripts/parity.sh` diffs these GETs between the two apps (§7 step 2) and reports 0 differences:

- `/api/research/desk`, `/api/research/compare`, `/api/research/calibration`, `/api/research/packs`, `/api/research/rules`, `/api/research/filings`, `/api/research/universe`, `/api/research/lookup?q={T}`
- `/api/research/{T}`, `/api/research/{T}/financials?view=standalone|ttm|annual`, `/api/research/{T}/pack`, `/api/research/{T}/valuation`, `/api/research/{T}/prices`, `/api/research/{T}/structure`, `/api/research/{T}/claims`, `/api/research/{T}/journal`, `/api/research/{T}/proposals`, `/api/research/{T}/calendar`, `/api/research/{T}/documents`
- By id, taken from side A's list responses in the same run: `/api/research/facts/{filing_id}/{element}` (first fact in `/{T}/financials?view=annual`), `/api/research/claims/{id}/evidence` and `/api/research/claims/{id}/evals` (first claim of `{T}`), `/api/research/journal/{id}` (first journal entry of `{T}`), `/api/thesis/drafts/{id}` (first draft)
- `/api/finance/stock-portfolio`, `/api/thesis/summary`, `/api/thesis/macro`, `/api/thesis/{T}`, `/api/thesis/drafts`, `/api/thesis/drafts/vault`, `/api/macro/observations`, `/api/macro/registry`, `/api/macro/scoring`
- `/api/macro/stream` is an event stream, not diffed: status 200 and `content-type: text/event-stream` on both sides. No screen reads it (D12).

Fields that legitimately differ are dropped by a named jq filter in `scripts/parity.sh`, each with its reason: request time (`as_of` of `/universe`, `/lookup`, `/{T}/valuation`; `generated_at` of `/thesis/summary|macro|{T}`; `summary.updated_at` of stock-portfolio; `as_of`, `score_vintage` of `/macro/scoring`; `as_of` of `/macro/observations`), process load time (`status.loaded_at` of `/packs`, `loaded_at` of `/rules`), and checkout path (`status.source`, `status.dir` of `/packs`, `source` of `/rules`, which hold an absolute directory when packs load from disk). Anything else that differs is a bug.

**A2 Counts.** `scripts/counts.sh` per S2 reports identical numbers on both snapshot copies at L4, and on `~/finance-app-data` vs `~/investing-app-data` after the final copy at L5.

**A3 Writes work in the new app** (sandbox data only, `scripts/writes.sh`): KPI cell PUT and DELETE, document upload and proposal accept/reject, claim create/patch, journal add and annotate, event dismiss, structure draft import and node accept/reject, universe add/override, packs reload, research refresh, thesis draft create and delete, macro trigger. Writes moved without a listed acceptance join A3 too (D19): scenarios PUT, `seen` POST, manual shares PUT/DELETE, manual calendar POST/DELETE, document link POST, evidence POST, structure node POST/DELETE, universe DELETE. Each returns the same status as the second finance-app instance for the same request. Research refresh returns 503 "Refresh unavailable" on both while the scheduler is off (finance-app `src/research/api.rs:1206-1208`); its 202 is checked on the live investing-app at L5 (plan.md §3.5).

**A4 Navigation.** Four bottom tabs, no FAB; each tab keeps its own scroll and last sub-route; a pushed screen belongs to the tab it was opened from and its back control (≥ 44 px) names that tab; deep links `#/perusahaan/:T?section=<block>` scroll to the block; old finance-app hash routes are not supported (no redirects). Details: design.md §2.

**A5 Phone floor.** S3 on every route in `design.md` §2.2; the structure map, Keuangan tables and Harga charts on a real held ticker (BUMI, BRIS) are checked at L4 on the sandbox.

**A6 Owner sign-off.** S5.

**A7 Removal.** S6, in a finance-app PR merged by the owner after A1–A6.

## 9. Open questions

Until the owner answers an open question, the slice builds the default written here, keeps its PR draft and says so in the PR body (D17). A different answer is a follow-up commit on the same PR.

- [ ] **O1 Thesis without research.** A thesis dossier or draft can exist for a ticker that is not in the research universe. Default: Perusahaan lists them in a collapsed group "Tesis lain (n)" (with vault drafts that have no ticker as "Draf vault (n)", D10); tapping a ticker opens the company page with only the header and the Tesis block, plus **Tambah ke riset**. Confirm at L3b review.
- [ ] **O2 PWA name and icon.** Default: name and short name "Investasi", glyph "I" on the shared accent, so the two Home Screen icons differ by letter and label. Confirm at L2 review.
- [x] **O3 Unused backend routes.** `/api/thesis/macro`, `/api/macro/scoring`, `/api/macro/registry` have no screen today. Decided 2026-10-27 (D19, default chosen by Hermes): moved unchanged (copy, not prune) and diffed in A1; pruning is a later, separate decision.
- [x] **O4 Keeping finance-app research data.** Decided 2026-10-27 (D18, default chosen by Hermes): after cutover, finance-app's research files stay **writable and untouched** in `~/finance-app-data` for 30 days, because rollback (§7 step 5) writes to them. The owner deletes them after that, in the L6 PR or later.

### Decisions 2026-10-27

Defaults chosen by Hermes after the canon QA; the owner may override any of them in PR review. D8–D16 (navigation, screens, e2e, phone floor) are written in design.md and plan.md §3.2–§3.3.

- **D1** `127.0.0.1:8789` and `~/investing-app-data` appear only in `src/config.rs` (+ tests), `deploy/`, `scripts/{parity,counts,writes}.sh`, `web/vite.config.ts`; finance-app's live values only in `scripts/`; `/Users/<name>` nowhere outside `*.md`.
- **D2** Parity, counts and writes compare investing-app with a second finance-app instance (release build of `4ef2dd9`) on a second copy of the same snapshot, spare port, throwaway token; live `:8787` is never a parity target.
- **D3** Snapshots: `sqlite3 .backup` for `research.sqlite`, then copies of the research files in §6 only; never `idempotency.jsonl`, spending files or `-wal`/`-shm` from a live directory; at L5 both services are stopped (owner) before the final copy.
- **D4** Every sandbox or smoke start sets `INVESTING_RESEARCH_DISABLE_SCHEDULER=1` and `INVESTING_MACRO_WATCHER=0` before its first start; L5 turns both on and sets `FINANCE_MACRO_WATCHER=0` on finance-app; no variable points under `~/finance-app-data`.
- **D5** `deploy/install.sh` installs only `com.menganto.investing`; `deploy/install-scraper.sh` installs the scraper job, run only at L5 (owner); install scripts never read Mission Control's `.env`, seed `macro_store.json`, create the env file or print a token.
- **D6** `scripts/parity.sh`, `scripts/counts.sh` and `scripts/writes.sh` are written and tested against two fixture data dirs in slice L1b, after L1; L4 only runs them.
- **D7** L1 drops `FINANCE_SPREADSHEET_ID`; renames the crate, the extractor binary literal, log filter and message prefixes; keeps vault paths, Yahoo URLs, `/api/finance/stock-portfolio`, `MACRO_DISABLE_FRED` and fixtures byte for byte; adds `"sandbox": true` to `/api/health` under `INVESTING_SANDBOX=1`; commits a regenerated `Cargo.lock`.
- **D17** Agent commits as `Menganto <fauzibrevy@gmail.com>` via `GIT_AUTHOR_*`/`GIT_COMMITTER_*`, branch `slice/<id>-<slug>` cut from current `main`, draft PR into `main`, screenshots under `docs/qa/<slice>/`; an unanswered owner question means build the default and keep the PR draft.
- **D18** finance-app research files are kept writable and untouched for 30 days after L5; the rollback path is written step by step (plan.md §3.5).
- **D19** A3 refresh is expected to return 503 in the scheduler-off sandbox and is checked at L5; S1 covers every GET listed in A1, which now includes every moved GET; every moved write is in A3.
- **D20** L1 copies `.claude/commands/draft-structure.md` pointed at `$INVESTING_DATA_DIR`; L6 deletes it from finance-app; L6's delete list is complete (plan.md §3.6) and S6 names checks that need neither Mission Control nor a live Sheet write.

## 10. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Owner edits research in investing-app during side by side and expects it after cutover | Medium | Medium: edits lost at cutover | §7 step 1 says it on the unlock screen while `INVESTING_SANDBOX=1`; the final snapshot is taken from finance-app after both services stop |
| Owner edits research in finance-app between the last check and the switch | Low | Medium | §7 step 4 stops both services before the final copy; `counts.sh` after the copy |
| Two writers after cutover: finance-app's scheduler, macro watcher or scraper job keeps running | Low | High | Step 4 sets both finance-app flags off, removes finance-app's scraper plist from `~/Library/LaunchAgents` (a bootout alone is undone at next login), installs investing-app's scraper only then; L4 never installs a scraper (D5) |
| Sandbox drifts from the snapshot (scheduler, macro watcher) and parity stays red | Medium | Low | Both off from the first start (D4); parity against a second finance-app instance on the same snapshot, not live `:8787` (D2) |
| A copy taken while finance-app writes is torn (DB and log disagree) | Medium | Medium | `sqlite3 .backup` for the DB; final copy with both services stopped (D3) |
| An env path left pointing into `~/finance-app-data` makes investing-app write finance-app's live files | Low | High | §6 rule; plan.md §3.4 step 1 check `grep -c finance-app-data ~/.hermes/secrets/investing-app.env` = 0 |
| Copy misses a file the build or runtime needs (untracked asset, pack TOML, fixture, deploy file read by tests) | Medium | Medium | Exact copy list in plan.md §3.1; L1/L2 done only when a fresh clone passes `scripts/check.sh` |
| Cookie or service-worker cross-talk between the two apps on one host | Medium | Medium: unlock loops, stale shell | Different cookie name and CSRF header; different port = different origin for the service worker and storage; tested on the iPhone at L4 |
| Parity diff stays red because of request-time or per-checkout fields | High | Low | Named jq drops with reasons (§8 A1); anything else that differs is a bug |
| Scope creep into redesign while moving | Medium | Medium | §4.2; screen content changes go to a later brief |
