# Runbook: investing-app

The Rust backend and the built UI run as one user LaunchAgent, `com.menganto.investing`, on
`127.0.0.1:8789` (Tailscale on 8445). Data lives in `~/investing-app-data`; the env file (owner-written,
mode 600, never committed) is `~/.secrets/investing-app.env`. Nothing here is run by CI, and
none of it touches finance-app's service (`com.menganto.finance`, `:8787`) or its data.

## 1. Install or update

The env file (`~/.secrets/investing-app.env`, or `INVESTING_ENV_FILE`; it moved there on 2026-11-12,
and the live plist also sets `INVESTING_ENV_FILE` to it) must exist first (plan.md §3.4 step 1) with
at least `INVESTING_APP_TOKEN` and `PORTFOLIO_SPREADSHEET_ID`; `INVESTING_BIND` is optional (default
`127.0.0.1:8789`). The installer never creates or edits it and never prints its values.

```
cd ~/investing-app && git pull && ./deploy/install.sh
```

It builds the release binary and the UI, copies both into a new release under the install root and
points `current` at it, creates the data directory, renders `deploy/com.menganto.investing.plist`
into `~/Library/LaunchAgents/` (launchd expands neither `$HOME` nor `~`, so the paths are filled in),
reloads the job and waits until `/api/health` answers 200 from a process started from the new
release, then requires `/` 200. The weekly scraper job is a separate step: §5, `install-scraper.sh`,
once, at cutover.

The service runs from the install root, never from the checkout, so `cargo clean`, a deleted
`target/` or `web/dist`, or a rebuild in `~/investing-app` cannot take it down:

```
~/.local/opt/investing-app/
  releases/<UTC-ts>-<shortsha>/   bin/investing-app, web/dist/, run.sh, RELEASE (sha, build time)
  current  -> releases/<id>       what the plist starts (current/run.sh)
  previous -> releases/<id>       what §4 rollback goes back to
```

The port is checked in preflight, before anything is built or written, and again right before the
switch. A port held by another process, a failed copy or check, or an invalid plist stops the
install before the switch: `current`, the plist in `~/Library/LaunchAgents` and the running job stay
as they were, and the staged release is removed. The switch itself is atomic (a new symlink renamed
over `current`), and only then is the plist moved into place and the job reloaded. After a healthy
start it keeps 3 releases (current, previous and the newest other) and deletes the rest. A release id
ending in `-dirty` was built from a checkout with uncommitted changes.

| Variable | Default | Why you would set it |
| --- | --- | --- |
| `INVESTING_INSTALL_ROOT` | `~/.local/opt/investing-app` | where releases live; absolute path |
| `INVESTING_LABEL` | `com.menganto.investing` | a spare instance (§2.1): only `com.menganto.investing.<name>` (not `.scraper`); needs its own install root and env file, and that env file its own `INVESTING_DATA_DIR` and `INVESTING_BIND` |
| `INVESTING_ENV_FILE` | `~/.secrets/investing-app.env` | a throwaway env file for a trial run |
| `SKIP_BUILD` | `0` | install the existing `target/release` and `web/dist` as a new release without building |

Research packs are the one thing still read from the checkout, on purpose: the plist sets
`INVESTING_RESEARCH_PACKS_DIR` to `<checkout>/research/packs` of the checkout `install.sh` ran from,
unless the env file sets it. Without an absolute path launchd starts the service in `/`, where the
relative default finds no pack files and `GET /api/research/packs` would report `files: 0` and only
`embedded_ids`. So install from the canonical clone, not a worktree. If the env file sets
`INVESTING_WEB_DIST`, `install.sh` warns: it overrides the UI of the release.

First start: macOS asks whether `investing-app` may access files in Documents, because the vault lives
in `~/Documents`. Click Allow. Until you do, `/api/thesis/*` answers nothing (curl status `000`, so
`scripts/parity.sh` shows `DIFF` on every thesis path) while `/api/health` still answers 200.

macOS ties that Documents permission to the binary's code identity, so a rebuild by `install.sh` can ask again (the same happens at L5). If `/api/thesis/*` hangs after a rebuild, check System Settings → Privacy & Security → Files & Folders before anything else.

**First install with the install root (once).** Until it runs, the service still starts
`~/investing-app/deploy/run.sh`, which keeps starting the checkout's own build, so a `git pull` alone
changes nothing. The first `./deploy/install.sh` then creates `~/.local/opt/investing-app`,
re-renders the plist to start `current/run.sh` and restarts the service. The binary now starts from
a new path, so expect the Documents prompt above once more. Check with the `ps` line in §2. There is
no `previous` yet, so `--rollback` refuses after this first install; going back to the old layout is
`git checkout <old sha> && ./deploy/install.sh` (the old installer renders the old plist), after
which `~/.local/opt/investing-app` can be deleted. If this first install stops with `launchctl bootstrap failed`
(it already waits for the old job to unload and tries three times), the job is not loaded: run the
`launchctl bootstrap` command the message prints, then the §2 checks; if that fails too, go back to
the old layout as above.

Parity is order-sensitive: run §3.4 step 5 once, in order, on a fresh snapshot (step 2). Re-running `parity.sh` after `writes.sh` shows timestamp-only `DIFF`s from the writes themselves, not a real difference.

## 2. Verify

```
L=com.menganto.investing PORT=8789 ROOT=~/.local/opt/investing-app LOG=investing-app   # spare instance: §2.1
U="gui/$(id -u)"
curl -s http://127.0.0.1:$PORT/api/health; echo        # {"status":"ok",...}
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:$PORT/   # 200: the UI, not only health
launchctl print "$U/$L" | grep -E 'state =|pid =|runs =|last exit'
lsof -nP -iTCP:$PORT -sTCP:LISTEN
ps -o pid,lstart=,command= -p "$(lsof -nP -iTCP:$PORT -sTCP:LISTEN -t)"   # path must be under $ROOT/releases/
readlink "$ROOT/current" "$ROOT/previous"                                 # releases/<ts>-<sha>
curl -s -H "Authorization: Bearer $TOKEN" "http://127.0.0.1:$PORT/api/health?deep=1"   # research_* entries
tail -n 20 ~/Library/Logs/$LOG.err.log
```

Over Tailscale (once): `tailscale serve --bg --https=8445 http://127.0.0.1:8789`, then
`curl -sf https://<mac>.<tailnet>.ts.net:8445/api/health`. While `INVESTING_SANDBOX` is set in the
env file, health carries `"sandbox": true`.

### 2.1 Spare instance (staging) next to the live one

To try an install, a break or a rollback without touching `com.menganto.investing`. Run it from a
worktree (`~/dev-worktrees/investing-app/<slug>`), not from `~/investing-app`. It gets its own label,
plist, logs, port 8799, env file with dummy values, data dir and install root, and its env file keeps
the vault, the scraper paths and the credentials away from the real ones; never copy the real env file.
`install.sh` refuses any label outside `com.menganto.investing.*` (and the scraper's), and a
non-default label that shares the live install root, env file, data dir or port. Always finish with
step 5: a plist left in `~/Library/LaunchAgents` has `RunAtLoad` and starts again at the next login.

```bash
# 1. Temp dirs and dummy values
T=$(mktemp -d "${TMPDIR:-/tmp}/investing-staging.XXXXXX"); T=$(cd "$T" && pwd -P)
mkdir -p "$T/data" "$T/vault" "$T/scraper"; printf '{}\n' > "$T/credentials.json"
( umask 077; printf '%s\n' "INVESTING_APP_TOKEN=$(openssl rand -hex 32)" PORTFOLIO_SPREADSHEET_ID=staging-dummy \
    "GOOGLE_APPLICATION_CREDENTIALS=$T/credentials.json" INVESTING_BIND=127.0.0.1:8799 \
    "INVESTING_DATA_DIR=$T/data" "INVESTING_VAULT_ROOT=$T/vault" "INVESTING_IDX_SCRAPER_DIR=$T/scraper" \
    "INVESTING_IDX_DB_PATH=$T/scraper/idx.sqlite" "INVESTING_IDX_EMITENS_CSV=$T/scraper/emitens.csv" \
    INVESTING_RESEARCH_DISABLE_SCHEDULER=1 MACRO_DISABLE_FRED=1 INVESTING_MACRO_WATCHER=0 \
    INVESTING_SANDBOX=1 > "$T/investing.env" )
export INVESTING_LABEL=com.menganto.investing.staging INVESTING_INSTALL_ROOT="$T/opt" \
  INVESTING_ENV_FILE="$T/investing.env" INVESTING_DATA_DIR="$T/data"
L=$INVESTING_LABEL PORT=8799 ROOT=$INVESTING_INSTALL_ROOT LOG=$INVESTING_LABEL

# 2. Install, then run the §2 checks (without the L=… line; skip the deep-health line)
./deploy/install.sh

# 3. Break test: the running service must not need the checkout's build output
mv target/release/investing-app "$T/bin.away"; mv web/dist "$T/dist.away"
launchctl kickstart -k gui/$(id -u)/$L      # then the §2 checks: new pid, 200 and 200
mv "$T/bin.away" target/release/investing-app; mv "$T/dist.away" web/dist

# 4. Second install, then rollback (§4)
SKIP_BUILD=1 ./deploy/install.sh            # current = the new release, previous = the first
./deploy/install.sh --rollback              # current = the first again, 200 and 200

# 5. Remove it
launchctl bootout gui/$(id -u)/$L
rm -f ~/Library/LaunchAgents/$L.plist ~/Library/Logs/$L.out.log ~/Library/Logs/$L.err.log
unset INVESTING_LABEL INVESTING_INSTALL_ROOT INVESTING_ENV_FILE INVESTING_DATA_DIR
rm -rf "$T"
```

The `releases/`, `current` and `previous` logic itself is tested without launchd:
`bash deploy/install.test.sh` (fake artifacts in a temp dir; not part of `scripts/check.sh` or CI).

## 3. Restart and stop

```
U="gui/$(id -u)"
launchctl kickstart -k "$U/com.menganto.investing"   # restart; a new pid must hold the port
launchctl bootout "$U/com.menganto.investing"        # stop (the job has KeepAlive: bootout, not kill)
launchctl bootstrap "$U" ~/Library/LaunchAgents/com.menganto.investing.plist   # start again
```

A spare instance (§2.1) uses its own label and `~/Library/LaunchAgents/<label>.plist` instead; its
logs are `~/Library/Logs/<label>.out.log` and `.err.log`.

Stop the service before copying or restoring `research.sqlite` (§5 backup): a live WAL database
copied file by file can be inconsistent. After a stop, `curl -s -m 2 http://127.0.0.1:8789/api/health`
must fail.

## 4. Rollback

**The release that ran before the last install:**

```
cd ~/investing-app && ./deploy/install.sh --rollback
```

It points `current` at `previous` (and `previous` at the release it left, so running it again goes
forward), restarts the job with `kickstart -k` (or starts it when it is stopped), and fails unless `/api/health` and `/` return 200
from a process started from that release. It needs no build and nothing from the checkout except
the packs directory. For a spare instance, run it with the same `INVESTING_*` variables as its
install. It swaps code only: the env file and the data directory stay as they are, so a release that
changed the store schema needs its data step first (§5, "Riset redesign" and "Original filing from
PDF"). Older than `previous`: `ls ~/.local/opt/investing-app/releases` lists the kept ones; anything
else is a reinstall of that commit, `git checkout <sha> && ./deploy/install.sh`.

**Cutover.** Cutover and rollback to finance-app as the research writer are plan.md §3.5 ("Rollback"); follow it
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

The old binary stays in the install root: after the deploy it is `previous` (§4), which `./deploy/install.sh --rollback` starts.

**After deploy, once.** With the service still stopped, run `--reingest`, then start the service:

```
~/.local/opt/investing-app/current/bin/investing-app research-ingest --xbrl-dir ~/tools/idx-financial-scraper/data/XBRL --db ~/investing-app-data/research.sqlite --reingest
```

It changes the BRIS non-performing financing (NPF) scale only. Before, BRIS NPF rows sat at wrong scales under unit IDR; after, every NPF row is unit `pure` in percent points (0.33 to 2.42 on the rehearsal data). Non-NPF facts are unchanged; derived cells and restatement rows for NPF change as a side effect. The capex and profit-share sign fix needs no command: rules version 2026.10-r4 re-derives stored cells at startup.

**Structure drafts.** Claude Code drafts a company's structure from an annual report with the project command `.claude/commands/draft-structure.md` and saves `<data_dir>/research/structure/<TICKER>.draft.json` (`~/investing-app-data/research/structure/` live). On the company page, **Muat draf** loads it as proposed nodes; the owner accepts, edits or rejects each branch (Terima, Ubah, Tolak). A second load replaces every node that is not accepted; accepted nodes stay. The server enforces 40 nodes and depth 4, and refuses an invalid file whole.

**Rollback.** A bare binary swap leaves the old build serving `/` and health, but every `/api/research/*` call returns 503 "Research store unavailable" (log: "research.sqlite is schema v11, this build knows up to v10"). Do all three steps:

1. Stop the service: `launchctl bootout gui/$(id -u)/com.menganto.investing`.
2. Fix the data, one of:
   - Set the version back: `sqlite3 ~/investing-app-data/research.sqlite 'PRAGMA wal_checkpoint(TRUNCATE); PRAGMA user_version=10;'`. Data written since the deploy stays; the unused `structure_maps` table is harmless.
   - Restore the backup: move `research.sqlite` and its `-wal` and `-shm` aside together, copy the backup in, check `user_version` is 10. Changes since the deploy stay only in the log.
3. Start the old binary: `./deploy/install.sh --rollback` (§4) when it is still `previous`; it starts the stopped service from that release. Otherwise `git checkout <old sha> && ./deploy/install.sh`.

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
~/.local/opt/investing-app/current/bin/investing-app research-ingest --xbrl-dir ~/tools/idx-financial-scraper/data/XBRL --db ~/investing-app-data/research.sqlite --reingest
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

Then start the old build (`./deploy/install.sh --rollback`, §4) and run its `--reingest` once so the derived cells are rebuilt without Q1 (or, in
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
