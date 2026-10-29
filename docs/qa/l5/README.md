# L5 cutover outputs

Recorded on 2026-10-29 for plan.md §3.5 and the L5 row of §4. Files:

- [checks.txt](checks.txt): step 8 health and endpoint checks
- [counts.txt](counts.txt): step 4 counts comparison
- [watch.txt](watch.txt): step 9 one-hour watch summary
- [watch-ticks.txt](watch-ticks.txt): step 9 health ticks, one every five minutes
- [watch-errors-corrected.txt](watch-errors-corrected.txt): step 9 ERROR count after the filter fix

Note: in `watch.txt` the two data-dir paths are redacted to `<finance-app data dir>` and `<investing-app data dir>` for the D1 hygiene rule. Nothing else in the file changed.

## Who ran what

- Owner: steps 1, 6 and 7 (`launchctl` and `install-scraper`). Step 6 started at 09:28:11 WIB.
- Hermes, on the owner's go (2026-10-29): steps 2–5, 8 and 9.

## Step 4: counts

Exit 0, 0 differences. `files_documents` and `thesis_drafts` are `missing (0)` on both sides, because finance-app never created them. See [counts.txt](counts.txt).

## Step 8: checks

Every check passes. See [checks.txt](checks.txt):

- Health has no sandbox key on `:8789` and `:8445`.
- `/` returns 200 on both.
- Refresh for BUMI returns 202.
- finance-app returns 200 on `:8787` and `:8443`.
- The investing scraper is loaded. The finance scraper is not.

## Step 9: one-hour watch (09:28–10:28)

- Health is ok on all 12 ticks ([watch-ticks.txt](watch-ticks.txt)).
- [watch.txt](watch.txt) reports "3 new ERROR lines". That is a filter bug in the watch script: ANSI colour codes sit before the timestamp, so the filter misses the lines' time. The 3 lines are from 2026-10-28. They are the L4 sandbox `writes.sh` refresh 503s. Corrected count: **0** ([watch-errors-corrected.txt](watch-errors-corrected.txt)).
- Source runs after step 6 are recorded (portfolio and `xbrl_dir`, every 10 minutes).
- finance-app's `research-log.jsonl` and `macro_store.json` mtimes are unchanged.
- Every `research_*` source is ok except **`research_filings`**. One of 141 filings (`BUMI_2024_Q1`) is reported as an unreadable duplicate after the data-dir move. Root cause and fix: commit fd8c925. The fix takes effect after the owner deploys this PR.

## Fixes found on live data

- **fd8c925**: a filing whose data dir moved follows its file. See the PR body.
- **7fc4d7d**: the weekday-dependent health test, plus a pinned-clock seam in production.
- **ee06ade**: the derive property. A comparative of exactly −original is a sign conflict, per the canon.

## Kept until L6 and 30 days after (D18)

- `~/finance-app-data-backup-20261029`
- `~/investing-app-data.sandbox-20261029`
- finance-app research files
- `*.env.pre-l5-20261029` env copies
