# plan.md — Investing App

Status: draft v0 for owner review · Owner: Menganto · Date: 2026-10-27
Requirements: [`prd.md`](prd.md). Screens: [`design.md`](design.md).

## 1. Approach

- **Clean copy, not a fork.** Code is copied from a clean checkout of `fauzibrevy-hub/finance-app` at a pinned commit (`4ef2dd9` unless the owner names a later one at L1 start), by an explicit file list (§3.1) or import closure (§3.2–§3.3), into this repo with fresh history. Each copying PR states the source commit. finance-app's history stays in finance-app.
- **Move first, change nothing.** L1–L3 change names, routes and navigation only. Screen content, formulas and endpoint JSON stay byte-for-byte what finance-app serves, so parity is a diff. The one behaviour L1 adds is `"sandbox": true` on `/api/health` under `INVESTING_SANDBOX=1` (D7).
- **finance-app is untouched until L6.** It stays the live app and the live research writer through the side-by-side period.
- **Who does what.** Hermes writes briefs, dispatches, and checks each slice itself before releasing the next. Code is written by Claude Code or omp (both read `CLAUDE.md`, omp through the `AGENTS.md` symlink). Every slice is a **draft PR** into `main`; the owner reviews and merges.
- **Agent workflow (D17).** Commit with `GIT_AUTHOR_NAME=Menganto GIT_AUTHOR_EMAIL=fauzibrevy@gmail.com GIT_COMMITTER_NAME=Menganto GIT_COMMITTER_EMAIL=fauzibrevy@gmail.com` in the environment, never a Co-Authored-By trailer, never touching git config. Branch `slice/<id>-<slug>` (names in §2), cut from current `main` after the slice's `depends_on` merged; no stacked bases. Screenshots and check output that do not fit the PR body are committed under `docs/qa/<slice>/` and linked from it (`gh` cannot upload images). An owner question still open at review time: build the default written in prd.md §9, keep the PR draft, say so in the PR body.
- **Live data rule.** Agents never read or write `~/finance-app-data`, `~/investing-app-data` or `~/.hermes/secrets`, never call `:8787`, `:8789` or `:8445`, never run `launchctl` or `tailscale`. Steps marked **(owner)** in §3.4–§3.5 are run by the owner in their own terminal (the Hermes guard refuses `launchctl bootstrap`, and tokens stay with the owner); the agent prepares the commands and records the output the owner pastes into the slice's PR. L1 and L1b need no live data: they run on scratch directories and `tests/fixtures/`.
- **Defaults (D1).** `127.0.0.1:8789` and `~/investing-app-data` are written only in `src/config.rs` (+ its tests), `deploy/`, `scripts/{parity,counts,writes}.sh` and `web/vite.config.ts`; finance-app's live values (`finance-app-data`, `127.0.0.1:8787`) only in `scripts/`; `/Users/<name>` nowhere outside `*.md`. `scripts/check.sh` hygiene enforces it (guardrails PR).
- **Snapshot (D3).** Every copy of finance-app research data uses this block, defined once and referred to as `snapshot` below. Tested 2026-10-27 on a scratch WAL database: the copy holds the DB rows, no `-wal`/`-shm`, no `idempotency.jsonl`, no `scraper.lock`, and a second run into an existing target refuses.

  ```bash
  # snapshot SRC DST [DRAFTS]: research files only (prd.md §6). DST must not exist.
  # DRAFTS = finance-app's FINANCE_THESIS_DRAFTS_PATH when its env sets one (owner looks it up).
  snapshot() {
    local src="$1" dst="$2" drafts="${3:-$1/thesis_drafts.json}"
    [ ! -e "$dst" ] || { echo "snapshot: $dst exists" >&2; return 1; }
    mkdir -p "$dst"
    sqlite3 "$src/research.sqlite" ".backup '$dst/research.sqlite'"
    cp -p "$src/research-log.jsonl" "$src/research-idempotency.jsonl" "$src/macro_store.json" "$dst/"
    cp -p "$drafts" "$dst/thesis_drafts.json"
    rsync -a --exclude scraper.lock "$src/research/" "$dst/research/"
  }
  ```

## 2. Slices

| # | Slice | Branch | depends_on | Contents | Done when |
|---|---|---|---|---|---|
| L0 | Canon | `docs/canon-v1-backend-ops`, `docs/canon-v1-web`, `chore/guardrails-v2` | — | `prd.md`, `design.md`, `plan.md`, `README.md`, `CLAUDE.md`, `scripts/check.sh` (D1 allowlist), hooks, CI | Owner merges all three canon PRs |
| L1 | Backend move | `slice/l1-backend-move` | L0 merged (L1 cannot pass hygiene before the D1 allowlist lands) | §3.1 | §4 L1 green |
| L1b | Tools | `slice/l1b-tools` | L1 merged | §3.1b | §4 L1b green |
| L2 | Web shell | `slice/l2-web-shell` | L1 merged (web tests import `tests/fixtures/research/**`) | §3.2 | §4 L2 checks green; prd.md O2 answered or its default built (§1) |
| L3a | Beranda + Alat | `slice/l3a-beranda-alat` | L2 merged | §3.3 | §4 L3 checks green for these routes |
| L3b | Perusahaan + company page + Tesis block | `slice/l3b-perusahaan` | L3a merged | §3.3 | §4 L3 checks green; prd.md O1 answered or its default built (§1) |
| L3c | Portofolio | `slice/l3c-portofolio` | L3a merged | §3.3 | §4 L3 checks green |
| L4 | Side by side | `slice/l4-side-by-side` (holds `docs/qa/l4/` only) | L1b, L3b, L3c merged | §3.4 | §4 L4 green; owner uses it alongside finance-app |
| L5 | Cutover | `slice/l5-cutover` (holds `docs/qa/l5/` only) | L4 merged and the owner's go written in the L5 PR | §3.5 | §4 L5 green; one-hour watch clean |
| L6 | Removal from finance-app | finance-app repo, `slice/l6-remove-investing` | L5 done and prd.md A1–A6 met | §3.6 | §4 L6 green; finance-app PR merged by the owner |

Order: L1 → (L1b ∥ L2) → L3a → (L3b ∥ L3c) → L4 → L5 → L6. A slice starts only when every `depends_on` PR is merged, not merely green. Owner gates: every PR merge, the L4 iPhone review, the L5 go.

## 3. Slice contents

### 3.1 L1 Backend move

`REF` = a clean checkout of finance-app at `4ef2dd9`. Paths below are relative to `REF` and keep their path here unless a target is named. Evidence for every line: QA `l1-backend` (trial B with this list built 522 tests green on macOS).

**Copy unchanged, then apply the renames below**

| From `REF` | Note |
|---|---|
| `Cargo.toml`, `Cargo.lock` | Repo root. Package renamed `investing-app`; dependency list unchanged. Then `cargo update --workspace --offline` once: the lock diff is only the `finance-app` → `investing-app` package block (measured 2026-10-27). This lock change is in scope for L1 |
| `src/{main,lib,auth,config,error,health,http,idempotency,py,query}.rs` | `lib.rs` edited as below |
| `src/research/**`, `src/thesis/**`, `src/macro_watch/**`, `src/sheets/**` | Includes `include_str!` inputs `src/research/store/migrations/*.sql`, `src/macro_watch/registry.json` |
| `research/rules.toml`, `research/packs/*.toml` (7 files) | `include_str!` inputs (`src/research/valuation.rs:25`, `src/research/packs/builtin.rs:19-43`) and `INVESTING_RESEARCH_PACKS_DIR` default |
| `tests/research_*.rs` (32 files), `tests/macro_watch.rs`, `tests/auth.rs`, `tests/sheets_client.rs`, `tests/thesis.rs`, `tests/static_cache.rs`, `tests/common/mod.rs` | `thesis.rs` is the only regression for `/api/thesis/*`; `static_cache.rs` covers `static_cache_headers` in `lib.rs` |
| `tests/support/{pack_fixture,r4_env,xbrl_fixture,mock_sheets}.rs` | Pulled in by `#[path = "support/…"]` (e.g. `tests/research_api.rs:6-10`); `mock_sheets.rs` serves `tests/portfolio.rs` |
| `tests/fixtures/{research,macro,pdf-filings,thesis_golden,vault}/` | Byte for byte. `tests/fixtures/research/**` is copied whole because the web Vitest suite imports it (88 matching lines in 30 files under `REF/web/src`, e.g. `features/research/canon.golden.test.ts`). There is no `sheets` fixture directory |
| `deploy/scraper.sh`, `deploy/run.sh`, `deploy/com.menganto.finance.plist` → `deploy/com.menganto.investing.plist`, `deploy/com.menganto.finance.scraper.plist` → `deploy/com.menganto.investing.scraper.plist` | `cargo test` inputs: `tests/research_health.rs:160-162,330` runs `deploy/scraper.sh` against a stub, `:575` reads the scraper plist (path follows the rename). Nothing is installed or loaded by launchd in L1 |
| `.claude/commands/draft-structure.md` | Line 10: `$FINANCE_DATA_DIR` (default `~/finance-app-data`) → `$INVESTING_DATA_DIR` (default `~/investing-app-data`) (D20) |
| `scripts/golden/{macro,thesis}_golden.py` | Provenance of the goldens (`tests/macro_watch.rs:1-2`); not run by any gate |

**Split `src/finance/portfolio.rs` → `src/portfolio.rs`** (no `FinanceState`, no `idempotency.jsonl`, no finance spreadsheet)

- `use super::parse::{py_round, to_rupiah};` (`:14`) → `use crate::py::py_round;` and copy `to_rupiah` (`src/finance/parse.rs:97-99`) into `src/portfolio.rs`.
- `use super::FinanceState;` (`:15`) → new `pub struct PortfolioState { pub sheets: SheetsClient, pub portfolio_spreadsheet_id: String, reads: TtlCache<Arc<Rows>> }` in `src/portfolio.rs`, `READ_TTL` 45 s (`src/finance/mod.rs:28`), with `rows_in` copied from `src/finance/mod.rs:119-133`. `stock_portfolio` takes `State<Arc<PortfolioState>>`.
- `pub fn router(cfg: Arc<Config>) -> Router` builds `SheetsClient::new(cfg.google_credentials_path.clone())` as `src/finance/mod.rs:249` does; `pub fn router_with(state: Arc<PortfolioState>) -> Router` mounts only `GET /api/finance/stock-portfolio`.
- `src/research/sources/portfolio.rs:7` → `use crate::portfolio::{parse_portfolio, PORTFOLIO_RANGE};`
- `src/lib.rs`: drop `pub mod finance;` (`:7`), `pub mod idx;` (`:11`) and their merges (`:67`, `:69`); add `pub mod portfolio;` and `.merge(portfolio::router(cfg.clone()))`.
- New `tests/portfolio.rs`: `stock_portfolio_reads_portfolio_spreadsheet` moved from `tests/finance_read.rs:417-461` with the helpers it needs (`rows`, a `MockData` holding only the portfolio range, `app_with` building `PortfolioState` over `support/mock_sheets.rs` as `tests/finance_read.rs:168-178` does), so the route's 200 and 500 paths stay tested. The parse unit tests stay inside `src/portfolio.rs`.

**Not copied:** the rest of `src/finance/` (`mod.rs`, `parse.rs`, `read.rs`, `write.rs`, `ledger.rs`), `src/idx.rs`, `tests/finance_read.rs` (except the moved test), `tests/finance_write.rs`, `tests/idx.rs`, `scripts/parity.sh` (Mission Control vs finance-app; L1b writes a new one), `deploy/install.sh` and `deploy/RUNBOOK.md` (rewritten below), `L1-BRIEF.md`, `docs/`, `web/` (L2).

**Renames** (scoped; never a blind `finance` → `investing` sweep)

| From | To | Where |
|---|---|---|
| package `finance-app`, crate `finance_app` | `investing-app`, `investing_app` | `Cargo.toml`, `Cargo.lock` (via `cargo update`), `src/main.rs`, every file under `tests/` that names the crate |
| log filter `finance_app=info` | `investing_app=info` | `src/main.rs:216` (unrenamed, the filter matches no target) |
| extractor binary `finance-app{EXE_SUFFIX}` | `investing-app{EXE_SUFFIX}` | `src/research/extract/isolated.rs:84` (unrenamed: 26 extract tests fail with 503) |
| message prefixes `finance-app:`, `finance research-ingest:` | `investing-app:`, `investing research-ingest:` | `src/main.rs` |
| `FINANCE_APP_TOKEN`, `_BIND`, `_DATA_DIR`, `_WEB_DIST`, `_VAULT_ROOT`, `_THESIS_DRAFTS_PATH`, `_IDX_DB_PATH`, `_MACRO_STORE_PATH`, `_MACRO_WATCHER`, `_RESEARCH_DB_PATH`, `_RESEARCH_WATCHLIST`, `_IDX_EMITENS_CSV`, `_IDX_SCRAPER_DIR`, `_IDX_XBRL_DIR`, `_PDF_FILINGS_DIR`, `_RESEARCH_PACKS_DIR`, `_RESEARCH_DISABLE_SCHEDULER`, `_EXTRACT_EXE` | `INVESTING_*` with the same suffix | `src/config.rs`, `src/research/extract/isolated.rs:76`, tests, `deploy/` |
| `FINANCE_ENV_FILE`, `FINANCE_APP_ROOT` (deploy only) | `INVESTING_ENV_FILE`, `INVESTING_APP_ROOT` | `deploy/run.sh`, `deploy/install.sh` |
| cookie `finance_session`, header `X-Finance-Client` / `x-finance-client` | `investing_session`, `X-Investing-Client` / `x-investing-client` | `src/auth.rs:5,7,30,32,139,234`, `src/research/api.rs:2`, `tests/auth.rs` |
| default bind `127.0.0.1:8787`, data dir `finance-app-data` | `127.0.0.1:8789`, `investing-app-data` | `src/config.rs:89,96,213,232` and its unit tests, `src/lib.rs:2`, `deploy/` |
| labels `com.menganto.finance(.scraper)`, logs `finance-app.*.log`, `finance-scraper.*.log` | `com.menganto.investing(.scraper)`, `investing-app.*.log`, `investing-scraper.*.log` | `deploy/`, `tests/research_health.rs:575` |

`X-Finance-Cache-Date` exists only in `web/` and is renamed in L2 (§3.2). `INVESTING_IDX_DB_PATH` is kept: health still reports `idx.sqlite` age (`src/health.rs:54`, `src/research/health.rs:121`).

**Dropped:** `FINANCE_SPREADSHEET_ID` / `finance_spreadsheet_id` entirely (`src/config.rs:22,104-105,167` and every test env list that sets it, e.g. `:264-273`); investing-app never reads the finance sheet (D7). `MC_ROOT` and every Mission Control read (D5).

**Must not rename:** vault paths `finance/stock`, `finance/article-bank`, `finance/thesis` (`src/thesis/mod.rs:56-64`, `src/thesis/lens.rs:14-15`, and `src/thesis/drafts.rs:421` which is API JSON); Yahoo URLs `…finance.yahoo.com/v8/finance/chart` (`src/research/prices/yahoo.rs:22`, `src/macro_watch/upstream.rs:33`); the path `/api/finance/stock-portfolio`; `MACRO_DISABLE_FRED`, `PORTFOLIO_SPREADSHEET_ID`, `GOOGLE_APPLICATION_CREDENTIALS` and the default credentials file name (`src/config.rs:108-111`); anything under `tests/fixtures/` (hashed bytes, e.g. the XBRL comment "Synthetic fixture for finance-app tests").

**Added (D7):** `INVESTING_SANDBOX` (truthy, default off) → `Config::sandbox`; `GET /api/health` returns `"sandbox": true` in both its plain (`src/health.rs:73`) and deep form while it is set, and no `sandbox` key otherwise; one test for each case.

**`deploy/` (D5).** Templates keep `__HOME__` / `__APP_ROOT__` placeholders rendered by `sed` at install (REF `install.sh:111-112`).

- `run.sh`: REF `deploy/run.sh` renamed; defaults `INVESTING_ENV_FILE=$HOME/.hermes/secrets/investing-app.env`, `INVESTING_BIND=127.0.0.1:8789`, `INVESTING_DATA_DIR=$HOME/investing-app-data`, binary `target/release/investing-app`; exits 78 on an unreadable env file as REF `:11-14`.
- `install.sh`: REF `install.sh` minus the scraper (`:16-17`, `:117-124`, `:129-130`), minus Mission Control (`:19`, `:33-42`), minus the macro seed (`:56-67`; `mkdir -p "$DATA_DIR"` stays), minus env-file creation, token printing and env rewriting (`:69-107`). Instead: `[ -f "$ENV_FILE" ] || fail "env file missing: $ENV_FILE (owner writes it: plan.md §3.4 step 1)"`, `chmod 600`, and `grep -q '^INVESTING_APP_TOKEN=' "$ENV_FILE"` and `grep -q '^PORTFOLIO_SPREADSHEET_ID=' "$ENV_FILE"` without printing values. `BIND` comes from the env file's `INVESTING_BIND` when present, else `127.0.0.1:8789`. Builds, renders and loads only `com.menganto.investing`, waits for health and `/` as REF `:133-148`, and ends with `tailscale serve --bg --https=8445 http://127.0.0.1:8789`.
- `install-scraper.sh` (new): refuses while `launchctl print "gui/$(id -u)/com.menganto.finance.scraper"` succeeds (two scrapers would share one scraper checkout); renders `com.menganto.investing.scraper.plist` (REF `install.sh:120-123`), `plutil -lint`, bootout and bootstrap of that label only. Run once, by the owner, at L5 (§3.5 step 7).
- `scraper.sh`: REF with `FINANCE_DATA_DIR`, `FINANCE_IDX_SCRAPER_DIR`, `FINANCE_RESEARCH_WATCHLIST`, `FINANCE_IDX_XBRL_DIR` → `INVESTING_*` (`:21-25`), default data dir `$HOME/investing-app-data`.
- `RUNBOOK.md`: REF `deploy/RUNBOOK.md` §8 "Riset: data sources and the scraper job" with its subsections (`:141` to the end: backup and restore, store v11 and v12 notes, `--reingest`, structure drafts, rules review) renamed for investing-app, plus short new sections: install (`install.sh`; `install-scraper.sh` only at cutover), verify, restart/stop, and rollback (pointer to plan.md §3.5).
- `install.sh` is not run in L1 (it needs `web/`, REF `:30,49,52`). Its first run is L4 step 3.

**Order of work**

1. Copy per the tables; commit "copy from finance-app 4ef2dd9" (no edits), so later diffs show every change.
2. Renames, drops, the portfolio split, `lib.rs`, `tests/portfolio.rs`, `INVESTING_SANDBOX`, `deploy/` rewrite.
3. `cargo update --workspace --offline`, then `cargo fmt` (the crate rename re-sorts imports in about 12 files).
4. `scripts/check.sh` (hygiene + backend), then the smoke below.

**Smoke (D4).** Scratch `HOME` and data dir, spare port, throwaway token, nothing under the real home, no live data. `bash -n` checked 2026-10-27; expected results are the ones QA l1-backend #19 observed on the same setup.

```bash
cargo build --release --locked
S="$(mktemp -d)"; mkdir -p "$S/data"; T="$(openssl rand -hex 32)"; U=http://127.0.0.1:18799
env -i PATH="$PATH" HOME="$S" \
  INVESTING_APP_TOKEN="$T" INVESTING_BIND=127.0.0.1:18799 INVESTING_DATA_DIR="$S/data" \
  PORTFOLIO_SPREADSHEET_ID=smoke GOOGLE_APPLICATION_CREDENTIALS="$S/none.json" MACRO_DISABLE_FRED=1 \
  INVESTING_RESEARCH_DISABLE_SCHEDULER=1 INVESTING_MACRO_WATCHER=0 INVESTING_SANDBOX=1 \
  ./target/release/investing-app >"$S/app.log" 2>&1 &
PID=$!
for _ in $(seq 40); do curl -sf -m 1 "$U/api/health" >/dev/null && break; sleep 0.25; done
curl -s "$U/api/health" | jq -e '.status == "ok" and .sandbox == true'
code() { curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $T" "$U$1"; }
echo "desk $(code /api/research/desk) stock $(code /api/finance/stock-portfolio) summary $(code /api/finance/summary) idx $(code /api/idx/companies)"
ls "$S/data"; grep -c 'finance_app\|finance-app' "$S/app.log"
kill "$PID"
```

Expected: `true`; `desk 200 stock 500 summary 404 idx 404` (stock 500 carries `"Failed to fetch stock portfolio: …"`: no credentials in the scratch home); `ls` shows only `research-idempotency.jsonl`, `research.sqlite`, `research.sqlite-shm`, `research.sqlite-wal`; `grep -c` prints `0`.

### 3.1b L1b Tools (D6)

Three scripts L4 and L5 only run, plus a self-test that proves them on fixtures. No defaults point at a live port; each refuses `:8787` before any request (D2).

**`scripts/counts.sh DIR_A DIR_B`** (prd.md S2, A2). Opens each `research.sqlite` read-only (`sqlite3 -readonly "file:$DIR/research.sqlite?mode=ro"`), writes nothing. Prints `metric<TAB>key<TAB>a<TAB>b` for each line that differs, then `counts: <n> differences`; exit 0 when n = 0, 1 otherwise, 2 on usage or a missing file. Drafts file: `$DIR/thesis_drafts.json`, overridable with `DRAFTS_A` / `DRAFTS_B`. The per-ticker query, run on a schema built from every finance-app migration on 2026-10-27:

```sql
SELECT 'filings', ticker, count(*) FROM filings GROUP BY ticker
UNION ALL SELECT 'facts', f.ticker, count(*) FROM facts x JOIN filings f ON f.id = x.filing_id GROUP BY f.ticker
UNION ALL SELECT 'kpi_values', ticker, count(*) FROM kpi_values GROUP BY ticker
UNION ALL SELECT 'claims', ticker, count(*) FROM claims GROUP BY ticker
UNION ALL SELECT 'claim_evals', c.ticker, count(*) FROM claim_evals e JOIN claims c ON c.id = e.claim_id GROUP BY c.ticker
UNION ALL SELECT 'journal', ticker, count(*) FROM journal GROUP BY ticker
UNION ALL SELECT 'journal_annotations', j.ticker, count(*) FROM journal_annotations a JOIN journal j ON j.id = a.journal_id GROUP BY j.ticker
UNION ALL SELECT 'documents', ticker, count(*) FROM documents GROUP BY ticker
UNION ALL SELECT 'evidence', d.ticker, count(*) FROM evidence e JOIN documents d ON d.id = e.document_id GROUP BY d.ticker
UNION ALL SELECT 'extraction_proposals', ticker, count(*) FROM extraction_proposals GROUP BY ticker
UNION ALL SELECT 'structure_nodes', ticker, sum(json_array_length(doc)) FROM structure_maps GROUP BY ticker
UNION ALL SELECT 'shares_manual', ticker, count(*) FROM shares_manual GROUP BY ticker
UNION ALL SELECT 'scenarios', ticker, count(*) FROM scenarios GROUP BY ticker
UNION ALL SELECT 'calendar_manual', ticker, count(*) FROM calendar_manual GROUP BY ticker
UNION ALL SELECT 'events', coalesce(ticker, '-'), count(*) FROM events GROUP BY ticker
UNION ALL SELECT 'events_dismissed', coalesce(ticker, '-'), count(*) FROM events WHERE dismissed_at IS NOT NULL GROUP BY ticker
UNION ALL SELECT 'universe', ticker || ':' || role, 1 FROM universe
UNION ALL SELECT 'seen', ticker, 1 FROM seen
ORDER BY 1, 2;
```

`universe_overrides` is not counted: migration `005_drop_v4.sql:10` drops it. Whole-directory lines: `wc -l` of `research-log.jsonl` and `research-idempotency.jsonl`; `jq length` of the drafts file (an object keyed by draft id, finance-app `src/thesis/drafts.rs:40-55`, `src/thesis/mod.rs:155-156`); `jq length` of `macro_store.json` (indicators) and `jq '[.[] | (.history // []) | length] | add // 0'` (observations; 15 and 5 on `tests/fixtures/macro/store.json`); `find -type f | wc -l` of `research/documents`, `research/structure`, `research/pdf-filings`.

**`scripts/parity.sh`** (prd.md S1, A1). Env `A_URL A_TOKEN B_URL B_TOKEN` (required, no defaults; A = finance-app side), optional `TICKERS` (default: every `role == "held"` ticker in A's `/api/research/universe` `.items[]`, plus the first `role == "peer"`). Bearer on both sides. Each body goes through `jq -S` with the number rule from finance-app `scripts/parity.sh:28` (`NUM`) and the drop for its path; prints `ok <path>` or `DIFF <path>` with the first 40 lines of `diff`; a different status on the two sides is a `DIFF`. Paths: prd.md A1 in order; ids for the by-id paths come from A's list responses in the same run; facts use `first(.. | objects | select(.filing_id and .element))` of `/{T}/financials?view=annual`. `/api/macro/stream`: status and `content-type` only (`curl -m 3`). Exit 0 when nothing differs, 1 otherwise, 2 on usage or a refused URL. Named drops, each written in the script with its reason:

| Path | jq drop | Reason (finance-app at `4ef2dd9`) |
|---|---|---|
| `/api/research/packs` | `del(.status.loaded_at, .status.source, .status.dir)` | process load time; absolute packs directory of each checkout (`src/research/api.rs:895-905`, `src/research/packs/builtin.rs:137-148`) |
| `/api/research/rules` | `del(.loaded_at, .source)` | same (`src/research/valuation.rs:121-139`) |
| `/api/research/universe`, `/api/research/lookup?q=…` | `del(.as_of)` | request time (`src/research/api.rs:963`, `src/research/api/lookup.rs:110`) |
| `/api/research/{T}/valuation` | `del(.as_of)` | request time (`src/research/valuation.rs:882`) |
| `/api/finance/stock-portfolio` | `del(.summary.updated_at)` | request time (`scripts/parity.sh:68`) |
| `/api/thesis/summary`, `/api/thesis/macro`, `/api/thesis/{T}` | `del(.generated_at)` | request time (`scripts/parity.sh:69`) |
| `/api/macro/observations` | `del(.as_of)` | request time (`scripts/parity.sh:70`) |
| `/api/macro/scoring` | `del(.as_of, .score_vintage)` | request time (`scripts/parity.sh:71`) |

L1b confirms each jq path against the self-test run and removes any drop that matches nothing; a new drop needs a reason line and the owner's OK in the L1b PR.

**`scripts/writes.sh`** (prd.md A3). Env `A_URL A_TOKEN A_DATA B_URL B_TOKEN B_DATA TICKER`. Refuses `:8787`, and refuses unless B's `/api/health` has `"sandbox": true`. Sends each request to A, then to B, each side using the ids its own earlier responses returned; request bodies are copied from the finance-app test named, so the expected status is known. Prints `label<TAB>a<TAB>b<TAB>ok|DIFF`; exit 0 when every pair matches.

| Request | Body from finance-app `tests/` |
|---|---|
| `POST /api/research/universe`, `PUT /api/research/universe/{T}`, `DELETE /api/research/universe/{T}` (a second, added ticker) | `research_universe_api.rs` |
| `PUT` and `DELETE /api/research/{T}/kpi/{metric}/{fy}/{q}` | `research_kpi.rs` |
| `POST /api/research/{T}/documents/upload`, `POST /api/research/proposals/{id}/accept`, `…/reject` | `research_extract.rs` |
| `POST /api/research/{T}/documents`, `POST /api/research/evidence` | `research_documents.rs` |
| `POST /api/research/{T}/claims`, `PATCH /api/research/claims/{id}` | `research_claims.rs` |
| `POST /api/research/{T}/journal`, `POST /api/research/journal/{id}/annotations` | `research_journal.rs` |
| `POST /api/research/events/{id}/dismiss` (first undismissed event of the desk) | `research_events.rs` |
| `POST /api/research/{T}/structure/draft` (after copying `tests/fixtures/research/structure-BUMI.draft.json` to `<DATA>/research/structure/{T}.draft.json` on each side), `PUT` (accepted, then rejected) and `DELETE /api/research/{T}/structure/nodes/{id}`, `POST /api/research/{T}/structure/nodes` | `research_structure.rs` |
| `PUT /api/research/{T}/scenarios` | `research_valuation.rs` |
| `POST /api/research/{T}/seen`, `PUT` and `DELETE /api/research/{T}/shares` | `research_api.rs` |
| `POST /api/research/{T}/calendar`, `DELETE /api/research/{T}/calendar/{date}/{kind}` | `research_calendar.rs` |
| `POST /api/research/packs/reload` | `research_packs.rs` |
| `POST /api/research/refresh?ticker={T}` (expected 503 on both while the scheduler is off, D19) | `research_api.rs` |
| `POST /api/thesis/drafts`, `DELETE /api/thesis/drafts/{id}` | `thesis.rs` |
| `POST /api/macro/trigger` | `macro_watch.rs` |

**`scripts/tools-selftest.sh`** proves the three on fixtures, with no live data and no network beyond what `macro/trigger` does with `MACRO_DISABLE_FRED=1`:

1. `cargo build --release --locked`; `W="$(mktemp -d)"`.
2. Fixture dir `F1`: `./target/release/investing-app research-ingest --xbrl-dir tests/fixtures/research/xbrl --db "$W/f1/research.sqlite"` (tickers AAAA, BBBB, CCCC); `cp tests/fixtures/macro/store.json "$W/f1/macro_store.json"`; `echo '{}' > "$W/f1/thesis_drafts.json"`; empty `research-log.jsonl`, `research-idempotency.jsonl`; `mkdir -p "$W/f1/research/"{documents,structure,pdf-filings}`.
3. `F2` = `snapshot "$W/f1" "$W/f2"` (the §1 block, so the self-test exercises it too).
4. `scripts/counts.sh "$W/f1" "$W/f2"` → exit 0, last line `counts: 0 differences`.
5. Start the binary twice with the smoke env (§3.1): A on F1 at `127.0.0.1:18801`, B on F2 at `127.0.0.1:18802` with `INVESTING_SANDBOX=1`; add AAAA as a manual ticker on both with the request from `research_universe_api.rs`.
6. `TICKERS=AAAA scripts/parity.sh` → exit 0, no `DIFF`.
7. Negative: one extra claim on B only → `parity.sh` exit 1 naming `/api/research/AAAA/claims`; `counts.sh "$W/f1" "$W/f2"` exit 1 naming `claims AAAA`.
8. `TICKER=AAAA scripts/writes.sh` → exit 0; the refresh row reads `503 503 ok`.
9. Guards: `A_URL=http://127.0.0.1:8787 scripts/parity.sh` and the same for `writes.sh` exit 2 before any request; `writes.sh` with B started without `INVESTING_SANDBOX` exits 2.
10. Stop both binaries; the script exits 0 only when every step above matched.

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

Runs on the owner's Mac. **(owner)** = owner's own terminal (§1). Order matters: the env file with the sandbox flags exists before anything starts (D4), and no scraper job is installed (D5).

0. **(owner)** `git -C ~/investing-app pull --ff-only` on `main`; `lsof -nP -iTCP:8789 -sTCP:LISTEN` prints nothing; `tailscale serve status` shows no `:8445`.
1. **(owner)** Write `~/.hermes/secrets/investing-app.env` (mode 600). Values in `<…>` are copied from `~/.hermes/secrets/finance-app.env`; lines whose finance-app counterpart is unset are left out.

   ```
   INVESTING_APP_TOKEN=<new: openssl rand -hex 32; never pasted into a PR or chat>
   PORTFOLIO_SPREADSHEET_ID=<PORTFOLIO_SPREADSHEET_ID>
   GOOGLE_APPLICATION_CREDENTIALS=<GOOGLE_APPLICATION_CREDENTIALS>
   INVESTING_VAULT_ROOT=<FINANCE_VAULT_ROOT>
   INVESTING_IDX_SCRAPER_DIR=<FINANCE_IDX_SCRAPER_DIR>
   INVESTING_IDX_XBRL_DIR=<FINANCE_IDX_XBRL_DIR>
   INVESTING_BIND=127.0.0.1:8789
   INVESTING_DATA_DIR=~/investing-app-data
   INVESTING_RESEARCH_DISABLE_SCHEDULER=1
   INVESTING_MACRO_WATCHER=0
   INVESTING_SANDBOX=1
   ```

   Not set, so they resolve under `~/investing-app-data`: `INVESTING_THESIS_DRAFTS_PATH`, `_MACRO_STORE_PATH`, `_PDF_FILINGS_DIR`, `_RESEARCH_WATCHLIST`, `_RESEARCH_DB_PATH`. Check: `grep -c finance-app-data ~/.hermes/secrets/investing-app.env` prints `0`.
2. **(owner)** Snapshot, finance-app still running: `snapshot ~/finance-app-data ~/investing-app-data "$DRAFTS"` (`DRAFTS` = finance-app's `FINANCE_THESIS_DRAFTS_PATH`, or omit the argument when unset). Then the parity copy, from the snapshot before anything opens it: `mkdir ~/investing-app-parity && cp -c -R ~/investing-app-data ~/investing-app-parity/fa-data`. Baseline: `scripts/counts.sh ~/investing-app-parity/fa-data ~/investing-app-data` exit 0.
3. **(owner)** `cd ~/investing-app && ./deploy/install.sh` (installs and starts `com.menganto.investing` only), then `tailscale serve --bg --https=8445 http://127.0.0.1:8789`. `curl -s http://127.0.0.1:8789/api/health` shows `"sandbox":true`.
4. **(owner)** Second finance-app instance on the parity copy (D2):

   ```bash
   P=~/investing-app-parity
   git clone -q ~/finance-app "$P/fa-src" && git -C "$P/fa-src" checkout -q 4ef2dd9
   (cd "$P/fa-src" && cargo build --release --locked)
   (umask 077; openssl rand -hex 32 > "$P/token")
   (
     set -a; . ~/.hermes/secrets/finance-app.env; set +a
     unset FINANCE_THESIS_DRAFTS_PATH FINANCE_MACRO_STORE_PATH FINANCE_PDF_FILINGS_DIR \
           FINANCE_RESEARCH_WATCHLIST FINANCE_RESEARCH_DB_PATH FINANCE_RESEARCH_PACKS_DIR
     FINANCE_APP_TOKEN="$(cat "$P/token")"
     export FINANCE_APP_TOKEN FINANCE_BIND=127.0.0.1:18787 \
            FINANCE_DATA_DIR="$P/fa-data" FINANCE_RESEARCH_DISABLE_SCHEDULER=1 FINANCE_MACRO_WATCHER=0
     cd "$P/fa-src" && exec ./target/release/finance-app
   ) >"$P/fa.log" 2>&1 &
   ```

5. **(owner)** In one shell, in this order (writes last, since they change both copies). Outputs are committed under `docs/qa/l4/` in the L4 PR; the scripts never print tokens.

   ```bash
   A_TOKEN="$(cat "$HOME/investing-app-parity/token")"
   B_TOKEN="$(sed -n 's/^INVESTING_APP_TOKEN=//p' "$HOME/.hermes/secrets/investing-app.env")"
   export A_URL=http://127.0.0.1:18787 A_TOKEN A_DATA="$HOME/investing-app-parity/fa-data"
   export B_URL=http://127.0.0.1:8789 B_TOKEN B_DATA="$HOME/investing-app-data"
   scripts/parity.sh | tee docs/qa/l4/parity.txt
   scripts/counts.sh "$A_DATA" "$B_DATA" | tee docs/qa/l4/counts.txt
   TICKER=BUMI scripts/writes.sh | tee docs/qa/l4/writes.txt
   ```

6. **(owner)** Stop the second instance (`kill %1`, or `pkill -f investing-app-parity/fa-src/target/release/finance-app`) and `rm -rf ~/investing-app-parity`.
7. **(owner)** iPhone: install the PWA from `https://<mac>.<tailnet>.ts.net:8445`; the unlock screen shows the sandbox line; run `docs/iphone-checklist.md` on every route in design.md §2.2, with the structure map, Keuangan and Harga on BUMI and BRIS (prd.md A5); use the app alongside finance-app. Results and screenshots go to `docs/qa/l4/iphone.md`.

The sandbox keeps running, scheduler and watcher off, until L5. Anything the owner does in it is thrown away at L5 step 3.

### 3.5 L5 Cutover (after the owner's go)

All steps **(owner)**, in one shell with `U="gui/$(id -u)"`, `D="$(date +%Y%m%d)"`, `B_TOKEN` read as in §3.4 step 5, `DRAFTS` as in §3.4 step 2, and the §1 `snapshot` function defined.

1. Stop every writer, scraper first, and keep finance-app's scraper from coming back at login:

   ```bash
   launchctl bootout "$U/com.menganto.finance.scraper"
   launchctl bootout "$U/com.menganto.finance"
   launchctl bootout "$U/com.menganto.investing"
   mkdir -p ~/Library/LaunchAgents.disabled
   mv ~/Library/LaunchAgents/com.menganto.finance.scraper.plist ~/Library/LaunchAgents.disabled/
   curl -s -m 2 http://127.0.0.1:8787/api/health; curl -s -m 2 http://127.0.0.1:8789/api/health   # both fail
   ```

2. Backup: `snapshot ~/finance-app-data ~/finance-app-data-backup-"$D" "$DRAFTS"`.
3. Final copy: `mv ~/investing-app-data ~/investing-app-data.sandbox-"$D"`, then `snapshot ~/finance-app-data ~/investing-app-data "$DRAFTS"`.
4. `DRAFTS_A="${DRAFTS:-$HOME/finance-app-data/thesis_drafts.json}" scripts/counts.sh ~/finance-app-data ~/investing-app-data | tee docs/qa/l5/counts.txt` → exit 0.
5. Flags (D4): remove the sandbox lines from investing-app's env, turn finance-app's research writers off.

   ```bash
   sed -i '' '/^INVESTING_RESEARCH_DISABLE_SCHEDULER=/d;/^INVESTING_MACRO_WATCHER=/d;/^INVESTING_SANDBOX=/d' ~/.hermes/secrets/investing-app.env
   sed -i '' '/^FINANCE_RESEARCH_DISABLE_SCHEDULER=/d;/^FINANCE_MACRO_WATCHER=/d' ~/.hermes/secrets/finance-app.env
   printf '%s\n' FINANCE_RESEARCH_DISABLE_SCHEDULER=1 FINANCE_MACRO_WATCHER=0 >> ~/.hermes/secrets/finance-app.env
   ```

6. Start both: `launchctl bootstrap "$U" ~/Library/LaunchAgents/com.menganto.investing.plist` and `launchctl bootstrap "$U" ~/Library/LaunchAgents/com.menganto.finance.plist`. Note the time.
7. `cd ~/investing-app && ./deploy/install-scraper.sh` (refuses while finance-app's scraper is loaded).
8. Checks, outputs to `docs/qa/l5/checks.txt`:
   - `curl -s http://127.0.0.1:8789/api/health` → `{"status":"ok"}` with no `sandbox` key; `/` → 200; the same two on `https://<mac>.<tailnet>.ts.net:8445`.
   - Refresh, the A3 write the sandbox could not prove (D19): `curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $B_TOKEN" 'http://127.0.0.1:8789/api/research/refresh?ticker=BUMI'` → `202`.
   - `curl -s -m 2 http://127.0.0.1:8787/api/health` → 200; finance-app spending screens open on `:8443`.
   - `launchctl print "$U/com.menganto.investing.scraper" >/dev/null` succeeds and `launchctl print "$U/com.menganto.finance.scraper"` fails.
9. One-hour watch. Clean means all of:
   - no new line matching `ERROR|panicked` in `~/Library/Logs/investing-app.err.log` or `investing-app.out.log` since step 6;
   - `GET /api/health?deep=1` (Bearer) has every `research_*` source `ok`, except `research_scraper_job`, which shows finance-app's last Sunday run until investing-app's first one (copied `scraper-status.json`);
   - `sqlite3 -readonly ~/investing-app-data/research.sqlite 'select * from source_runs order by rowid desc limit 5'` shows a run started after step 6 (universe sync runs at start, finance-app `deploy/RUNBOOK.md:152`);
   - `~/finance-app-data/research-log.jsonl` and `~/finance-app-data/macro_store.json` keep the modification time they had at step 6 (`stat -f %m`): finance-app no longer writes research.
10. Keep `~/investing-app-data.sandbox-<D>`, `~/finance-app-data-backup-<D>` and every finance-app research file, writable and untouched, until L6 merges and 30 days have passed (D18).

**Rollback** (until L6 merges; D18). Puts finance-app back as the research writer with every investing-app edit.

```bash
U="gui/$(id -u)"; D="$(date +%Y%m%d)"; F=~/finance-app-data; I=~/investing-app-data
# R1 stop investing-app and its scraper, for good
launchctl bootout "$U/com.menganto.investing.scraper"; launchctl bootout "$U/com.menganto.investing"
mkdir -p ~/Library/LaunchAgents.disabled
mv ~/Library/LaunchAgents/com.menganto.investing.scraper.plist ~/Library/LaunchAgents/com.menganto.investing.plist ~/Library/LaunchAgents.disabled/
# R2 stop finance-app
launchctl bootout "$U/com.menganto.finance"
# R3 carry investing-app's data back; finance-app's DB is moved aside, not deleted
mkdir "$F/rollback-$D" && mv "$F"/research.sqlite* "$F/rollback-$D/"
cp -p "$I/research-log.jsonl" "$I/research-idempotency.jsonl" "$F/"
for d in documents structure pdf-filings; do rsync -a "$I/research/$d/" "$F/research/$d/"; done
cp -p "$I/thesis_drafts.json" "${DRAFTS:-$F/thesis_drafts.json}"
# R4 rebuild: a fresh DB beside the log replays the whole log on open, then facts are re-ingested
~/finance-app/target/release/finance-app research-ingest \
  --xbrl-dir "${FINANCE_IDX_XBRL_DIR:-$HOME/tools/idx-financial-scraper/data/XBRL}" --db "$F/research.sqlite" --reingest
# R5 finance-app's research writers back on
sed -i '' '/^FINANCE_RESEARCH_DISABLE_SCHEDULER=/d;/^FINANCE_MACRO_WATCHER=/d' ~/.hermes/secrets/finance-app.env
# R6 finance-app's scraper and service back
mv ~/Library/LaunchAgents.disabled/com.menganto.finance.scraper.plist ~/Library/LaunchAgents/
launchctl bootstrap "$U" ~/Library/LaunchAgents/com.menganto.finance.scraper.plist
launchctl bootstrap "$U" ~/Library/LaunchAgents/com.menganto.finance.plist
# R7 close investing-app's tailnet port; the owner uses :8443 again
tailscale serve --https=8445 off
```

Why R4 works: `Store::open` rebuilds a brand-new database from the log beside it (finance-app `src/research/store/mod.rs:555-557`, `:995-1020`), and `--reingest` rebuilds XBRL and PDF facts (finance-app `deploy/RUNBOOK.md:278-281`, `:321-325`). Check: `:8787/api/health` 200, and the owner's last investing-app edit (a claim, journal entry or KPI cell) shows on its company page in finance-app. Not carried back: macro observations investing-app fetched after cutover (finance-app's watcher refetches them).

### 3.6 L6 Removal from finance-app

A finance-app PR (its repo, its canon, branch `slice/l6-remove-investing`), after L5 and prd.md A1–A6. Line numbers are finance-app at `4ef2dd9`; the agent re-finds them on finance-app `main`.

**Delete**

- Backend: `src/research/`, `src/thesis/`, `src/macro_watch/`, `research/` (packs and `rules.toml`); `src/finance/portfolio.rs` and its route (`src/finance/mod.rs:276-279`) and `portfolio_spreadsheet_id` in `FinanceState` (`:89`, `:106`); the `research-ingest` CLI and the extractor subcommand in `src/main.rs` (`:5-195`, `:203-212`); the thesis, macro and research merges in `src/lib.rs`; the config fields and env reads of the variables below.
- Tests: `tests/research_*.rs`, `tests/macro_watch.rs`, `tests/thesis.rs`, `tests/support/{pack_fixture,r4_env,xbrl_fixture}.rs`, `tests/fixtures/{research,macro,pdf-filings,thesis_golden,vault}/`, `stock_portfolio_reads_portfolio_spreadsheet` in `tests/finance_read.rs`, the thesis cases in `tests/idx.rs`.
- Scripts and deploy: `scripts/golden/`, the research, thesis, macro and stock-portfolio paths in `scripts/parity.sh`, `.claude/commands/draft-structure.md` (D20), `deploy/scraper.sh`, `deploy/com.menganto.finance.scraper.plist`, the scraper steps of `deploy/install.sh` (`:16-17`, `:117-124`, `:129-130`), `deploy/RUNBOOK.md` §8 (`:141` to the end).
- Web: `web/src/features/research/`, `web/src/features/riset/`, `web/src/screens/{portfolio,thesis}/`, `web/e2e/research/`, the research cases in `web/src/lib/api.test.ts` and `web/src/pwa/swPolicy.test.ts`, the Investasi and Riset tabs (`web/src/shell/nav.ts:3,18-38`) and the `#/riset`, `#/research`, `#/idx` routes (`web/src/App.tsx:13-16,21-24,44-46`).
- Env vars (S6): `FINANCE_VAULT_ROOT`, `FINANCE_THESIS_DRAFTS_PATH`, `FINANCE_MACRO_STORE_PATH`, `FINANCE_MACRO_WATCHER`, `MACRO_DISABLE_FRED`, `FINANCE_RESEARCH_DB_PATH`, `FINANCE_RESEARCH_WATCHLIST`, `FINANCE_RESEARCH_PACKS_DIR`, `FINANCE_RESEARCH_DISABLE_SCHEDULER`, `FINANCE_IDX_EMITENS_CSV`, `FINANCE_IDX_SCRAPER_DIR`, `FINANCE_IDX_XBRL_DIR`, `FINANCE_PDF_FILINGS_DIR`, `FINANCE_EXTRACT_EXE`, and `PORTFOLIO_SPREADSHEET_ID` (read only by stock-portfolio and the research scheduler: `src/finance/portfolio.rs:249`, `src/research/mod.rs:115`). `FINANCE_IDX_DB_PATH` stays (`src/health.rs:54`, legacy `src/idx.rs:311`).
- Not in the PR: finance-app's research files in `~/finance-app-data`. They stay writable and untouched for 30 days after L5 (D18); the owner deletes them afterwards.

**Update:** finance-app `prd.md`, `design.md`, `plan.md`, `deploy/RUNBOOK.md`. finance-app's new navigation is decided in that PR, not here.

**Checks for S6**, none needing Mission Control or a live Sheet write:

- finance-app full gates (fmt, clippy, cargo test, tsc, vitest, build, e2e).
- The PR's release binary in a scratch home: `env -i PATH="$PATH" HOME="$S" FINANCE_APP_TOKEN="$(openssl rand -hex 32)" FINANCE_BIND=127.0.0.1:18797 FINANCE_DATA_DIR="$S/data" FINANCE_SPREADSHEET_ID=dummy ./target/release/finance-app` starts with no research variable set; `/api/health` 200; with Bearer, `/api/research/desk`, `/api/thesis/summary`, `/api/macro/observations`, `/api/finance/stock-portfolio` → 404; finance-app prd §3.1 A1.6 (POST without auth → 401) and A1.7 (`"account":"BCA"` or `"amount":0` → 422) pass.
- `git grep -nE 'api/(research|thesis|macro)|stock-portfolio' -- ':!*.md' ':!docs/'` and `git grep -nE 'FINANCE_(VAULT_ROOT|THESIS|MACRO|RESEARCH|PDF|EXTRACT|IDX_(EMITENS|SCRAPER|XBRL))|PORTFOLIO_SPREADSHEET_ID|MACRO_DISABLE_FRED' -- ':!*.md' ':!docs/'` both empty.
- **(owner)** after merge and deploy: spending screens on `:8443` show live data. A1.4, A1.5 and A1.9 (a live Sheet write) are optional and the owner's call.

## 4. Checks per slice

| Slice | Commands (run from a fresh clone of the PR branch) |
|---|---|
| L1 | `scripts/check.sh` exit 0: hygiene passes (D1 allowlist, no `FINANCE_`/`finance_session`/`X-Finance` outside `*.md` and `scripts/`), `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --no-fail-fast` with 0 failed (QA's trial of the §3.1 list ran 522) — the same script CI runs · `git diff <copy commit> -- Cargo.lock` shows only the package block · the §3.1 smoke with its expected output pasted in the PR |
| L1b | `bash -n` and `shellcheck` on `scripts/{parity,counts,writes,tools-selftest}.sh` · `scripts/check.sh` exit 0 · `scripts/tools-selftest.sh` exit 0, output pasted in the PR |
| L2 | `npm ci` · `npx tsc -b` · `npm test` · `npm run build` · Playwright WebKit iPhone 15: bar has 4 items, each ≥ 44 px, no FAB, no page sideways scroll on any placeholder route |
| L3 | L2 checks plus: e2e for every route in design.md §2.2 at 393 px (no page overflow, targets ≥ 44 px, axe no serious/critical); for the company page, the structure map, Keuangan and Harga on BUMI and BRIS from a sandbox clone; screenshots attached to the PR |
| L4 | §3.4 outputs in `docs/qa/l4/`: baseline `counts.sh` exit 0 (step 2) · `/api/health` on `:8789` has `"sandbox":true` · `parity.sh` exit 0 · `counts.sh` exit 0 · `writes.sh` exit 0 with refresh `503 503` · `docs/iphone-checklist.md` passes on the owner's iPhone 15 (D16) |
| L5 | §3.5 outputs in `docs/qa/l5/`: both health calls fail after step 1 · `counts.sh` exit 0 after the final copy · health and `/` 200 on `:8789` and `:8445` with no `sandbox` key · refresh 202 · investing scraper loaded, finance scraper not · one-hour watch clean (step 9) |
| L6 | §3.6 checks |

## 5. Rollback

| Stage | How to go back | Data at risk |
|---|---|---|
| L0 | Revert the canon PR | None |
| L1, L1b, L2, L3 | Close or revert the PR | None; nothing runs |
| L4 | **(owner)** `launchctl bootout "gui/$(id -u)/com.menganto.investing"`, `rm ~/Library/LaunchAgents/com.menganto.investing.plist`, `tailscale serve --https=8445 off`; stop the parity instance and delete `~/investing-app-parity` (§3.4 step 6); `~/investing-app-data` may be deleted | None; the sandbox is a snapshot and finance-app was never stopped |
| L5 | §3.5 Rollback R1–R7 | None of the owner's research edits (log, documents, structure, drafts and PDF filings are carried back); macro observations fetched by investing-app after cutover are refetched by finance-app |
| L6 | Revert the finance-app removal PR, then §3.5 Rollback | None while finance-app's research files are kept (30 days, D18) |
