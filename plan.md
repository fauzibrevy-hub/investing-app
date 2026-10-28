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

**`scripts/counts.sh DIR_A DIR_B`** (prd.md S2, A2). Opens each `research.sqlite` read-only (`sqlite3 -readonly "file:$DIR/research.sqlite?mode=ro"`), writes nothing. Prints `metric<TAB>key<TAB>a<TAB>b` for each line that differs, then `counts: <n> differences`; exit 0 when n = 0, 1 otherwise, 2 on usage or a missing file. Drafts file: `$DIR/thesis_drafts.json`, overridable with `DRAFTS_A` / `DRAFTS_B`. Finance-app creates the drafts file and `research/{documents,structure,pdf-filings}` only on first use (loader `src/thesis/drafts.rs:31` treats NotFound as an empty map), so on either side an absent drafts file counts as 0 drafts and an absent directory as 0 files, each printed as `note<TAB>side<TAB>metric<TAB>missing (0)` ahead of the differences (found at L4, 2026-10-28). A drafts file that exists but is corrupt, or a path of the wrong kind, still exits 2. The per-ticker query, run on a schema built from every finance-app migration on 2026-10-27:

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

`universe_overrides` is not counted: migration `005_drop_v4.sql:10` drops it. Whole-directory lines use key `-` and these metric labels:

| Metric | Value |
|---|---|
| `log_lines` | `wc -l < research-log.jsonl` |
| `idempotency_lines` | `wc -l < research-idempotency.jsonl` |
| `thesis_drafts` | `jq length` of the drafts file (an object keyed by draft id, finance-app `src/thesis/drafts.rs:40-55`, `src/thesis/mod.rs:155-156`) |
| `macro_indicators` | `jq length macro_store.json` (15 on `tests/fixtures/macro/store.json`) |
| `macro_observations` | `jq '[.[] \| (.history // []) \| length] \| add // 0' macro_store.json` (5 on the same fixture) |
| `files_documents`, `files_structure`, `files_pdf_filings` | `find research/<dir> -type f \| wc -l` for `documents`, `structure`, `pdf-filings` |

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

**`scripts/writes.sh`** (prd.md A3). Env `A_URL A_TOKEN A_DATA B_URL B_TOKEN B_DATA TICKER`. Refuses `:8787`, and refuses unless B's `/api/health` has `"sandbox": true` (unreachable, non-JSON, key absent or false all refuse, exit 2). A data dir that is or links to `~/finance-app-data` is refused on both sides, always. `~/investing-app-data` is refused as `A_DATA`; as `B_DATA` it is allowed only past that health check, because at L4 it IS the sandbox's own data dir (§3.4 step 5), thrown away at L5 step 3 (found at L4, 2026-10-28; the L1b guard refused it unconditionally). Sends each request to A, then to B, each side using the ids its own earlier responses returned; request bodies are copied from the finance-app test named, so the expected status is known. Prints `label<TAB>a<TAB>b<TAB>ok|DIFF|SKIP`; exit 0 when every pair matches.

Requests run in this order, so every id exists before a request uses it: whatever a step creates is updated or deleted only by a later step, using the id from that step's own response on the same side. Three steps take an id the script does not create itself (12, 13, 15). When that id is missing on both sides, the step prints `label<TAB>-<TAB>-<TAB>SKIP <reason>` and does not fail the run. When it is missing on one side only, the step is a `DIFF`.

| # | Request | Id from | Body from finance-app `tests/` |
|---|---|---|---|
| 1 | `POST /api/research/universe` (a second ticker `{U}`, role manual) | — | `research_universe_api.rs` |
| 2 | `PUT /api/research/universe/{T}` (override) | — | `research_universe_api.rs` |
| 3 | `DELETE /api/research/universe/{U}` | step 1 | `research_universe_api.rs` |
| 4 | `PUT /api/research/{T}/kpi/{metric}/{fy}/{q}` | — | `research_kpi.rs` |
| 5 | `DELETE` the same KPI cell | step 4 | `research_kpi.rs` |
| 6 | `POST /api/research/{T}/claims` | — | `research_claims.rs` |
| 7 | `PATCH /api/research/claims/{id}` | step 6 | `research_claims.rs` |
| 8 | `POST /api/research/{T}/documents` (link) | — | `research_documents.rs` |
| 9 | `POST /api/research/evidence` | claim from 6, document from 8 | `research_documents.rs` |
| 10 | `POST /api/research/{T}/journal` | — | `research_journal.rs` |
| 11 | `POST /api/research/journal/{id}/annotations` | step 10 | `research_journal.rs` |
| 12 | `POST /api/research/events/{id}/dismiss` | first undismissed event in `GET /api/research/desk`, read now (after the claim and journal writes); SKIP "no open event" | `research_events.rs` |
| 13 | `POST /api/research/{T}/documents/upload`, then `POST /api/research/proposals/{id}/accept` and `…/{id}/reject` | first and second pending proposal in `GET /api/research/{T}/proposals` after the upload; SKIP "fewer than 2 proposals" for the accept/reject pair only (the upload always runs) | `research_extract.rs` |
| 14 | `POST /api/research/{T}/structure/draft`, after writing the draft file on each side (below) | — | `research_structure.rs` |
| 15 | `PUT /api/research/{T}/structure/nodes/{id}` with `accepted`, then another node with `rejected` | first two node ids in `GET /api/research/{T}/structure` after step 14 | `research_structure.rs` |
| 16 | `POST /api/research/{T}/structure/nodes`, then `DELETE /api/research/{T}/structure/nodes/{id}` | the POST's own response | `research_structure.rs` |
| 17 | `PUT /api/research/{T}/scenarios` | — | `research_valuation.rs` |
| 18 | `POST /api/research/{T}/seen` | — | `research_api.rs` |
| 19 | `PUT /api/research/{T}/shares`, then `DELETE` it | the PUT | `research_api.rs` |
| 20 | `POST /api/research/{T}/calendar`, then `DELETE /api/research/{T}/calendar/{date}/{kind}` with the same date and kind | the POST | `research_calendar.rs` |
| 21 | `POST /api/research/packs/reload` | — | `research_packs.rs` |
| 22 | `POST /api/research/refresh?ticker={T}` (expected 503 on both while the scheduler is off, D19) | — | `research_api.rs` |
| 23 | `POST /api/thesis/drafts`, then `DELETE /api/thesis/drafts/{id}` | the POST | `thesis.rs` |
| 24 | `POST /api/macro/trigger` | — | `macro_watch.rs` |

Step 16 always creates its own node, so it never SKIPs. Step 15 SKIPs only if the import in step 14 failed on both sides, and then step 14 is already a `DIFF` or a matching error.

Draft file for step 14. The import refuses a file whose `ticker` is not `{T}` (`ticker_mismatch`, finance-app `tests/research_structure.rs:585-589`) and any node `metric` outside `{T}`'s pack or core (`unknown_metric`, `src/research/structure.rs:139-147`). The fixture is BUMI's, with coal-pack metrics `production_mt`, `sales_mt` and `asp_usd_t`. So each side gets a rewritten copy, and the fixture itself stays byte for byte:

```bash
jq --arg t "$TICKER" '.ticker = $t | if $t == "BUMI" then . else .nodes |= map(del(.metric)) end' \
  tests/fixtures/research/structure-BUMI.draft.json > "$DATA/research/structure/$TICKER.draft.json"
```

Checked 2026-10-27 on the fixture: `TICKER=AAAA` gives ticker AAAA, 40 nodes, no metrics; `TICKER=BUMI` keeps all three metrics.

**`scripts/tools-selftest.sh`** proves the three on fixtures, with no live data and no network beyond what `macro/trigger` does with `MACRO_DISABLE_FRED=1`:

1. `cargo build --release --locked`; `W="$(mktemp -d)"`.
2. Fixture dir `F1`: `./target/release/investing-app research-ingest --xbrl-dir tests/fixtures/research/xbrl --db "$W/f1/research.sqlite"` (tickers AAAA, BBBB, CCCC); `cp tests/fixtures/macro/store.json "$W/f1/macro_store.json"`; `echo '{}' > "$W/f1/thesis_drafts.json"`; empty `research-log.jsonl`, `research-idempotency.jsonl`; `mkdir -p "$W/f1/research/"{documents,structure,pdf-filings}`.
3. `F2` = `snapshot "$W/f1" "$W/f2"` (the §1 block, so the self-test exercises it too).
4. `scripts/counts.sh "$W/f1" "$W/f2"` → exit 0, last line `counts: 0 differences`.
5. Start the binary twice with the smoke env (§3.1): A on F1 at `127.0.0.1:18801`, B on F2 at `127.0.0.1:18802` with `INVESTING_SANDBOX=1`; add AAAA as a manual ticker on both with the request from `research_universe_api.rs`.
6. `TICKERS=AAAA scripts/parity.sh` → exit 0, no `DIFF`.
7. Negative: one extra claim on B only → `parity.sh` exit 1 naming `/api/research/AAAA/claims`; `counts.sh "$W/f1" "$W/f2"` exit 1 naming `claims AAAA`.
8. `TICKER=AAAA scripts/writes.sh` → exit 0; the refresh row reads `503 503 ok`; the structure-draft row reads `ok` (the AAAA rewrite above). `SKIP` is accepted only on steps 12 and 13 (the fixture desk may have no open event, and the upload may yield fewer than two proposals), and each `SKIP` line is pasted into the L1b PR. At L4, any `SKIP` is reported in the L4 PR next to `writes.txt`.
9. Guards: `A_URL=http://127.0.0.1:8787 scripts/parity.sh` and the same for `writes.sh` exit 2 before any request; `writes.sh` with B started without `INVESTING_SANDBOX` exits 2.
    Step 4 also covers never-created drafts and documents: 0 = 0 with `missing (0)` notes on both sides; one side without a drafts file against a side with a draft is a difference; a drafts path that is a directory exits 2.
    Step 9 also runs `writes.sh` under a curl shim and a scratch `$HOME`: `B_DATA` = `investing-app-data` with `"sandbox": true` passes the guard; with the key absent, `false`, non-JSON or B unreachable it exits 2 after the one health read; `finance-app-data` exits 2 with no request even when health says `sandbox: true`.
10. Stop both binaries; the script exits 0 only when every step above matched.

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
git grep -nE '(^|[^A-Za-z0-9_.-])/(riset|research|portfolio|thesis|idx)([/?"`$]|$)' -- web/src ':!*.test.*' ':!*.testutil.*' | grep -vE ':[0-9]+:[[:space:]]*(\*|//|/\*)'
```

Decision 2026-10-28 (owner): the pattern skips path segments such as `features/riset/` and fixture paths, so only route literals match.

On finance-app `4ef2dd9` the same command lists the literals above plus files that are not carried.

**Tests.** Carried with their files when their imports resolve. Ported, not dropped:
- `features/research/{vocabulary,provenance,display}.test.tsx` and `independence.test.ts` (no verdict words, provenance, display rules, no foreign API prefix) render `canonSurfaces.testutil.tsx` `SURFACES`, which mounts `ResearchRoutes` on `/research/*`. L3a rewrites `SURFACES` to mount `App`'s route table on the design.md §2.2 routes it lands (Beranda, the three tools); L3b adds the company page sections, Perusahaan and Tambah; L3c adds Portofolio.
- `features/riset/companyPage.test.tsx`, `features/riset/screens/screens.test.tsx`, `features/riset/leftovers.test.tsx` import `dossier/*`: repoint to the moved modules; cases that assert `#/riset` routes or desktop layout are rewritten for the new routes.
- `screens/portfolio/portfolio.test.tsx` (L3c) and `screens/thesis/thesis.test.tsx` (L3b): rewritten for the Sheet and the Tesis block.

Dropped, each listed by path in the lane's PR body with one line why: `features/research/routes.test.tsx` (tests `ResearchRoutes`, `IdxTickerRedirect`, finance-app `App.tsx` and `nav.ts`), every test under `features/research/dossier/`, `desk/{DeskScreen,MobileDesk}.test.tsx`, and any other case that only covers a desktop variant.

**e2e per lane** (in `webkit-iphone15`, mocks from §3.2): L3a `beranda.e2e.ts` (finance-app `macro.e2e.ts` cases on the Makro grid, `overlays.e2e.ts` inbox sheets), L3b `perusahaan.e2e.ts` (company page sections incl. Struktur, Keuangan and Harga on COAL and REST, the claim sheet and `FactDrawer` from `overlays.e2e.ts`, the long-name and 1e15 cases of `layout.e2e.ts`, Tesis block, Draf vault, `?ticker=` prefill), L3c `portofolio.e2e.ts` (position Sheet numbers, eye toggle, **Buka halaman perusahaan** → "‹ Portofolio"). L3c also adds the prd.md S4 reach test to `nav.e2e.ts`: every place one tap from the bar; a company page within 2 taps of Perusahaan, Portofolio and a Beranda inbox item.

### 3.4 L4 Side by side

Runs on the owner's Mac, from `~/investing-app`, in a shell where the §1 `snapshot` function is defined. **(owner)** = owner's own terminal (§1). Order matters: the env file with the sandbox flags exists before anything starts (D4), and no scraper job is installed (D5).

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
| L2 | `scripts/check.sh` OK (its `web` part is `npm ci && npm test && npm run build`; `build` runs `tsc -b`) · in `web/`: `npx playwright install webkit && npm run e2e` (project `webkit-iphone15`, mocks only, D15): `shell.e2e.ts` 4 links each ≥ 44 × 44 px, no FAB, also at 200 % root font; `floor.e2e.ts` every design.md §2.2 route (placeholders): no page overflow, targets ≥ 44 px, axe no serious/critical; `nav.e2e.ts` design.md §2.4 · screenshots committed under `docs/qa/l2/` and linked in the PR body (D17) · `docs/iphone-checklist.md` rewritten (not run) |
| L3 | L2 checks plus: `floor.e2e.ts` on the lane's real screens (mock data: canon goldens, thesis goldens, `e2e/fixtures/stock-portfolio.json`; no backend, no live data); the lane's spec (§3.3 "e2e per lane"); the ported guard tests pass in `npm test`; tests dropped are listed in the PR body; screenshots under `docs/qa/l3a/`, `docs/qa/l3b/`, `docs/qa/l3c/`. After L3c: the old-route `git grep` of §3.3 is empty. Real held tickers (BUMI, BRIS) are checked at L4, not here |
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
