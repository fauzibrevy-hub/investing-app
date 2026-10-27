# Runbook: investing-app

The Rust backend and the built UI run as one user LaunchAgent, `com.menganto.investing`, on
`127.0.0.1:8789` (Tailscale on 8445). Data lives in `~/investing-app-data`; the env file (owner-written,
mode 600, never committed) is `~/.hermes/secrets/investing-app.env`. Nothing here is run by CI, and
none of it touches finance-app's service (`com.menganto.finance`, `:8787`) or its data.

## 1. Install or update

The env file must exist first (plan.md §3.4 step 1) with at least `INVESTING_APP_TOKEN` and
`PORTFOLIO_SPREADSHEET_ID`; `INVESTING_BIND` is optional (default `127.0.0.1:8789`). The installer
never creates or edits it and never prints its values.

```
cd ~/investing-app && git pull && ./deploy/install.sh
```

It builds the release binary and the UI, creates the data directory, renders
`deploy/com.menganto.investing.plist` into `~/Library/LaunchAgents/` (`__HOME__` and `__APP_ROOT__`
are filled in; launchd expands neither `$HOME` nor `~`), reloads the job and waits for
`/api/health` and `/`. Install from the canonical clone, not a worktree: the plist points at that
checkout and the service dies when it is deleted. `SKIP_BUILD=1` skips the builds. The weekly
scraper job is a separate step: §5, `install-scraper.sh`, once, at cutover.

## 2. Verify

```
U="gui/$(id -u)"
curl -s http://127.0.0.1:8789/api/health        # {"status":"ok",...}
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8789/   # 200: the UI, not only health
launchctl print "$U/com.menganto.investing" | grep -E 'state =|runs =|last exit'
lsof -nP -iTCP:8789 -sTCP:LISTEN
ps -o pid,lstart=,command= -p "$(lsof -nP -iTCP:8789 -sTCP:LISTEN -t)"   # path must be the clone, not a worktree
curl -s -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:8789/api/health?deep=1"   # research_* entries
tail -n 20 ~/Library/Logs/investing-app.err.log
```

Over Tailscale (once): `tailscale serve --bg --https=8445 http://127.0.0.1:8789`, then
`curl -sf https://<mac>.<tailnet>.ts.net:8445/api/health`. While `INVESTING_SANDBOX` is set in the
env file, health carries `"sandbox": true`.

## 3. Restart and stop

```
U="gui/$(id -u)"
launchctl kickstart -k "$U/com.menganto.investing"   # restart; a new pid must hold the port
launchctl bootout "$U/com.menganto.investing"        # stop (the job has KeepAlive: bootout, not kill)
launchctl bootstrap "$U" ~/Library/LaunchAgents/com.menganto.investing.plist   # start again
```

Stop the service before copying or restoring `research.sqlite` (§5 backup): a live WAL database
copied file by file can be inconsistent. After a stop, `curl -s -m 2 http://127.0.0.1:8789/api/health`
must fail.

## 4. Rollback

Cutover and rollback to finance-app as the research writer are plan.md §3.5 ("Rollback"); follow it
there rather than from memory. In short: it stops `com.menganto.investing` and its scraper, boots
`com.menganto.finance` back, carries investing-app's data back into `~/finance-app-data` and rebuilds
finance-app's `research.sqlite` from the log. Before cutover nothing needs rolling back: stop this
service (§3) and finance-app is untouched. Rolling back a store schema change within investing-app is
in §5 ("Riset redesign", "Original filing from PDF").

## 5. Riset: data sources and the scraper job

The research feature keeps its own store, `~/investing-app-data/research.sqlite` (WAL), plus an
append-only `research-log.jsonl` beside it holding the data you typed (manual additions, manual
share counts, entry targets, notes). Delete `research.sqlite` and restart: the log is replayed.
Everything else is refetched.

What runs inside the app (off with `INVESTING_RESEARCH_DISABLE_SCHEDULER=1`):

| Job | When | Source |
| --- | --- | --- |
| Universe sync, rewrites `research/watchlist.csv` | start, then every 10 min | portfolio sheet (held) + manual additions |
| XBRL re-read | `idx.sqlite` mtime changed (checked every 10 min) | scraper DB, read-only |
| Prices + dividends/splits + `^JKSE` | once per trading day after 16:30 WIB | Yahoo chart (unofficial, 1 req/s) |

Check it: `curl -s -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:8789/api/health?deep=1"`
and read the `research_*` entries. `research_prices` goes red after 4 days without a good fetch,
`research_xbrl` after 14 days; `research_scraper_job` goes `missed` when a Sunday run left no status and `stale` after 14 days. A failed fetch never deletes stored data;
failures are rows in `source_runs` (`sqlite3 ~/investing-app-data/research.sqlite 'select * from source_runs order by rowid desc limit 10'`).

Weekly scraper job (R7.1) — installed by `install-scraper.sh`, with the same render-and-reload
pattern as the app service, so re-running it is safe (the plist is re-rendered from this checkout
and the job reloaded). `install.sh` does not install it. **Installing it is the owner's step, once,
at cutover (plan.md §3.5 step 7); nothing in the repo or in CI loads a LaunchAgent or touches
`~/Library/LaunchAgents`.** The script refuses while finance-app's scraper job is still loaded,
because both would share one scraper checkout.

Owner install (once, on the Mac that runs the app):

```
cd ~/investing-app && ./deploy/install-scraper.sh
```

Prerequisites, checked before the first Sunday:

1. The scraper checkout (`~/tools/idx-financial-scraper`, or `INVESTING_IDX_SCRAPER_DIR`) is on a
   revision with `--watchlist` and Chrome detection (scraper PR #5), and Chrome is installed.
2. The app has been started once, so `~/investing-app-data/research/watchlist.csv` exists (the
   universe sync writes it, peers included). `INVESTING_RESEARCH_WATCHLIST` overrides the path and
   the status file moves with it.
3. Dry run now instead of waiting for Sunday: `launchctl kickstart gui/$(id -u)/com.menganto.investing.scraper`,
   then watch `~/Library/Logs/investing-scraper.out.log`.

| Task | Command |
| --- | --- |
| Install / update | `./deploy/install-scraper.sh` (installs `com.menganto.investing.scraper` only) |
| Run once now | `launchctl kickstart gui/$(id -u)/com.menganto.investing.scraper` |
| Is it loaded | `launchctl print gui/$(id -u)/com.menganto.investing.scraper \| head -20` |
| Last run, raw | `cat ~/investing-app-data/research/scraper-status.json` |
| Logs | `~/Library/Logs/investing-scraper.out.log` and `investing-scraper.err.log` |
| Health | `curl -s -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:8789/api/health?deep=1"` → `sources.research_scraper_job` |

The job fires Sundays 03:00 local (`StartCalendarInterval`); there is no `RunAtLoad`, so it
does not run at install time — use `kickstart` for a manual run. A Mac that sleeps through
03:00 runs the job once when it wakes; a Mac that is off skips that week.

What `deploy/scraper.sh` does: preflight (watchlist and `run.py` present, else exit 78), then
`run.py --skip-master --watchlist <watchlist>` (step `download`), then `model/sqlite_export.py`
(step `export`). Every run, failed or not, writes `research/scraper-status.json`:

```
{"started_at":"…Z","finished_at":"…Z","exit_code":0,"step":"done"}
```

Fields besides the three above: `new_files`, the number of files the run added to the XBRL
directory (`0` is a normal week, shown as such and never a failure). The status is rewritten at
every step change, so a run killed with SIGKILL still names the step it died in. `SIGTERM`,
`SIGHUP` and `SIGINT` (e.g. `launchctl kill`) stop the scrape and record exit 143, 129 and 130.
A lock directory `research/scraper.lock/` keeps two runs apart: the second exits 75 at once
and leaves the status alone (a lock whose process is gone, or older than 12 h, is replaced).
A status that cannot be written (read-only directory, full disk) is a failure of the run:
stderr says `this run is NOT recorded`, the exit is 74 (or the run's own non-zero code).

`research_scraper_job` in health reads it. Last scheduled run = the most recent Sunday 03:00
local time (the plist schedule); the grace period after it is 6 h.

| `state` | `ok` | Meaning |
| --- | --- | --- |
| `ok` | yes | Exit 0, started since the last scheduled slot (or the slot is inside the grace period), not older than 14 days |
| `running` | yes | Started under 6 h ago, not finished |
| `failed` | no | Non-zero exit; `exit_code` and `step` say where |
| `missed` | no | Last status is older than the last scheduled run by more than the grace period: the job did not fire, or its status could not be written (unwritable directory, full disk) |
| `stale` | no | Last success older than exactly 14 days (checked by the hour, not by whole days) |
| `unfinished` | no | Started over 6 h ago and never finished: SIGKILL or crash |
| `clock_skew` | no | `started_at` or `finished_at` is more than 5 min in the future; ages are `null`. Check the Mac's clock |
| `unreadable` | no | Corrupt file |
| `no_status` | by `idx.sqlite` age | No run since the status write existed |

`age_days` and `age_hours` are never negative. Fields: `exit_code`, `step`, `new_files`,
`started_at`, `finished_at`, `last_at`, `last_scheduled_at`, `age_days`, `age_hours`,
`idx_age_days`. Any red entry turns the overall `status` to `degraded`.

Health completeness (R7.2): `research_store`, `research_prices`, `research_xbrl`,
`research_filings`, `research_facts`, `research_drivers`, `research_kpi` and
`research_scraper_job` each carry `ok`, `detail`, `last_at` (RFC 3339 or `null`) and `age_days`
(whole days, or `null` when never fetched or no store). `research_store.age_days` is the age of
its newest recorded source run (`null` while the store has none; the one-time v4 migration row
does not count); `research_filings` and `research_facts` use the last directory
ingest.

Failures to expect: `Chrome not found` or an unknown `--watchlist` flag in the err log means the
scraper checkout is behind (prerequisite 1); exit 78 with step `preflight` means the app never
wrote the watchlist or `INVESTING_IDX_SCRAPER_DIR` is wrong. The app keeps working on the existing
`idx.sqlite` either way.
Env overrides: `INVESTING_IDX_SCRAPER_DIR`, `INVESTING_RESEARCH_WATCHLIST`, `PYTHON`.

### Backup and restore: R8 documents

Uploaded PDF/PPTX evidence lives outside the database, in `~/investing-app-data/research/documents/`
(`<sha256>.<ext>`, the same bytes stored once; the folder appears on the first upload). The
`documents` and `extraction_proposals` rows live in `research.sqlite` and point at those files by
hash, so back up both together:

- Copy `~/investing-app-data/research/documents/` and `~/investing-app-data/research.sqlite` (stop the
  service first, §3; a live WAL database copied file by file can be inconsistent), plus `research-log.jsonl`.
- Restore: stop the service (§3), put the folder back at the same path and the database beside
  it, start the service. Dot-files in the folder are leftovers of an interrupted upload and can
  be deleted.
- Check: `/api/health?deep=1` entries `research_documents` (folder present and writable, file
  count, document rows) and `research_proposals` (pending count, last extraction). A document
  row whose file was not restored keeps its extracted text but cannot be re-read.

### Riset redesign (store v11)

Rolling back this release needs both a data step and the old binary; swapping the binary alone breaks Riset.

**Before deploy.** Migration 011 adds the table `structure_maps` (schema v11). It applies at service
start (`CREATE TABLE IF NOT EXISTS`, sub-second) and, unlike v4 to 005, takes no automatic backup. Stop
the service (§3), then:

```
mkdir -p ~/investing-app-data/pre-riset-redesign
sqlite3 ~/investing-app-data/research.sqlite ".backup '$HOME/investing-app-data/pre-riset-redesign/research.sqlite'"
cp ~/investing-app-data/research-log.jsonl ~/investing-app-data/pre-riset-redesign/
sqlite3 ~/investing-app-data/pre-riset-redesign/research.sqlite 'pragma integrity_check; pragma user_version'   # ok, 10
```

Keep the old binary (`target/release/investing-app`) too if you want a fast rollback.

**After deploy, once.** With the service still stopped, run `--reingest`, then start the service:

```
~/investing-app/target/release/investing-app research-ingest --xbrl-dir ~/tools/idx-financial-scraper/data/XBRL --db ~/investing-app-data/research.sqlite --reingest
```

It changes the BRIS non-performing financing (NPF) scale only. Before, BRIS NPF rows sat at wrong scales under unit IDR; after, every NPF row is unit `pure` in percent points (0.33 to 2.42 on the rehearsal data). Non-NPF facts are unchanged; derived cells and restatement rows for NPF change as a side effect. The capex and profit-share sign fix needs no command: rules version 2026.10-r4 re-derives stored cells at startup.

**Structure drafts.** Claude Code drafts a company's structure from an annual report with the project command `.claude/commands/draft-structure.md` and saves `<data_dir>/research/structure/<TICKER>.draft.json` (`~/investing-app-data/research/structure/` live). On the company page, **Muat draf** loads it as proposed nodes; the owner accepts, edits or rejects each branch (Terima, Ubah, Tolak). A second load replaces every node that is not accepted; accepted nodes stay. The server enforces 40 nodes and depth 4, and refuses an invalid file whole.

**Rollback.** A bare binary swap leaves the old build serving `/` and health, but every `/api/research/*` call returns 503 "Research store unavailable" (log: "research.sqlite is schema v11, this build knows up to v10"). Do all three steps:

1. Stop the service: `launchctl bootout gui/$(id -u)/com.menganto.investing`.
2. Fix the data, one of:
   - Set the version back: `sqlite3 ~/investing-app-data/research.sqlite 'PRAGMA wal_checkpoint(TRUNCATE); PRAGMA user_version=10;'`. Data written since the deploy stays; the unused `structure_maps` table is harmless.
   - Restore the backup: move `research.sqlite` and its `-wal` and `-shm` aside together, copy the backup in, check `user_version` is 10. Changes since the deploy stay only in the log.
3. Install the old binary (`git checkout <old sha> && ./deploy/install.sh`, or the saved old binary) and start the service.

Check: deep health shows `research_store` "schema v10", and a pack call returns 200. Only the PRAGMA route was rehearsed, on a copy; the file-restore commands and `install.sh` were not (finance-app's `install.sh`, in the build this section was first written for).

**FRED user agent.** FRED is fetched with its own HTTP client and the user agent `curl/8.7.1` (`FRED_UA`), because FRED drops browser-like agents. BIS and OECD keep `Mozilla/5.0`.

### Original filing from PDF (store v12)

IDX has no XBRL instance for BUMI 2024 Q1. The owner reads the PDF outside the app into one JSON file
and the app ingests that file next to the XBRL source (format and rules: `src/research/xbrl/pdf.rs`). The checked file for BUMI is committed as
`tests/fixtures/pdf-filings/BUMI_2024_Q1.json`; nothing writes into `~/investing-app-data` until you copy it.

**Before deploy.** Migration 012 adds `filings.source_kind` and `facts.page`/`facts.label` (three
`ALTER TABLE ADD COLUMN`, sub-second, no automatic backup). Stop the service (§3), then back up as for
the Riset redesign above (`pre-pdf-filings` instead of `pre-riset-redesign`; `user_version` reads 11).

**Copy the file, once.** On the deploy host, from the checkout:

```
mkdir -p ~/investing-app-data/research/pdf-filings
cp tests/fixtures/pdf-filings/BUMI_2024_Q1.json ~/investing-app-data/research/pdf-filings/
```

The default directory is `$INVESTING_DATA_DIR/research/pdf-filings`; `INVESTING_PDF_FILINGS_DIR` moves it.
The service reads it on every scheduler tick, so a file added later is picked up within ten minutes.

**After deploy, once.** With the service still stopped, a reingest rebuilds the facts of both sources and
re-derives every ticker; then start the service:

```
~/investing-app/target/release/investing-app research-ingest --xbrl-dir ~/tools/idx-financial-scraper/data/XBRL --db ~/investing-app-data/research.sqlite --reingest
```

It prints `pdf filings <dir>: 1 in store` and the usual reconciliation lines (rehearsal on a copy of the
live data: 0 unexplained failures and 15 restated years, before and after the file). `--pdf-dir DIR`
overrides the directory for one run. A file the app rejects is listed as `UNREADABLE <path>: <reason>` and
in deep health (`research_filings.unparsed`); nothing from it is stored.

**What to expect.** BUMI 2024Q1 equity, cash, net debt and reclamation provisions fill in, 2024Q2
standalone values fill in (revenue 595,841,018 − 311,015,873 = 284,825,145), and the TTM ratios for 2024Q2
to 2025Q1 fill in. The Q1 column of Keuangan carries a `PDF` badge; the fact drawer shows `PDF · hal. N` and
the label as printed; pack cells that rest on the file list their PDF pages in the origin panel. Health
shows `research_filings.pdf_filings`.

**When IDX publishes the XBRL.** Nothing to do: an XBRL filing of the same id always wins. On the next
pass it replaces the PDF facts (the JSON file stays in the folder and is counted as kept out, not as an error).

**Adding another PDF filing.** Copy the format of the BUMI file: one fact per element the packs read, current
period only, the 1-based PDF page of each fact, the label as printed, and `components` where IDX's tag is a
sum of printed lines. The element names and signs must be the ones that company's XBRL filings use; check
by extracting the same elements from a filing that has XBRL and comparing.

**Rollback.** A bare binary swap leaves every `/api/research/*` call at 503 ("schema v12, this build knows up
to v11"). Stop the service (§3), then remove the PDF filing's rows (an old build neither knows nor removes
them), set the version back and install the old binary:

```
sqlite3 ~/investing-app-data/research.sqlite "DELETE FROM facts WHERE filing_id IN (SELECT id FROM filings WHERE source_kind='pdf'); DELETE FROM filings WHERE source_kind='pdf'; PRAGMA wal_checkpoint(TRUNCATE); PRAGMA user_version=11;"
```

Then start the old build and run its `--reingest` once so the derived cells are rebuilt without Q1 (or, in
the same sqlite3 call before the version is set, also remove BUMI's derived and restatement rows: the old
build re-derives a ticker that has facts and no derived rows at start). The added columns are harmless to
the old build, and deploying the new build again later is safe: migration 012 skips a column the table
already has. Rehearsed on a copy of the live data with the new build and an empty PDF directory: filings
and facts back to the pre-file counts and BUMI's gap count back to its pre-file value. The old binary
itself and `install.sh` were not run.

**Removing or renaming a file.** Deleting a JSON file, or changing its `ticker`, `fy` or `period`, does not
remove the filing it already produced (XBRL files behave the same). Remove that filing by hand with the
service stopped (its `facts` rows first, then its `filings` row where `source_kind` is `pdf`), then run the
reingest above.

A JSON file that an XBRL filing of the same id has replaced is read again on every tick and ignored (the CLI
prints `SUPERSEDED <id>`); it can stay or be deleted.

### Rules-review report

Removed in R5-E1 with the v4 rules engine and snapshots; the R7 rules review replaces it.
