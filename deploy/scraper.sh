#!/bin/bash
# launchd entry point for the weekly XBRL scraper job (Riset, plan §6.3, R7.1).
#
# Runs the scraper from its own checkout against the watchlist the app writes from the research
# universe, then exports idx.sqlite. The scraper code is not modified. The app notices the new
# idx.sqlite by mtime (checked every 10 minutes) and re-reads it.
#
# Run status: every run, including one that fails in preflight, leaves a one-line JSON file
# beside the watchlist (research/scraper-status.json). `GET /api/health?deep=1` reads it for
# `research_scraper_job` (last run time, exit status, step, files fetched). Written atomically and
# again at every step change. TERM/HUP/INT record 143/129/130 and the step they hit; a run that
# is SIGKILLed leaves `finished_at: null` (health: "did not finish").
#
# Lock: `research/scraper.lock/` (mkdir; macOS has no flock). A second run exits 75 at once and
# leaves the status alone. A lock whose pid is dead, or older than 12 h, is stale and replaced.
#
# A status that cannot be written is a failure: the run says so on stderr and exits non-zero (74
# when everything else succeeded). Health then reports `missed`, never a stale green.
set -euo pipefail

: "${FINANCE_DATA_DIR:=$HOME/finance-app-data}"
: "${FINANCE_IDX_SCRAPER_DIR:=$HOME/tools/idx-financial-scraper}"
: "${PYTHON:=python3}"
WATCHLIST="${FINANCE_RESEARCH_WATCHLIST:-$FINANCE_DATA_DIR/research/watchlist.csv}"
XBRL_DIR="${FINANCE_IDX_XBRL_DIR:-$FINANCE_IDX_SCRAPER_DIR/data/XBRL}"
# Must stay beside the watchlist: the app derives this path from FINANCE_RESEARCH_WATCHLIST.
STATUS_FILE="$(dirname "$WATCHLIST")/scraper-status.json"
LOCKDIR="$(dirname "$STATUS_FILE")/scraper.lock"

now() { date -u +%Y-%m-%dT%H:%M:%SZ; }
STARTED_AT="$(now)"
STEP="preflight"
NEW_FILES=null # files this run added to the XBRL dir; 0 is a valid weekly result
CHILD=""

write_status() { # exit_code|null finished_at|null
  local code="$1" fin="$2" tmp
  [ "$fin" = "null" ] || fin="\"$fin\""
  tmp="$STATUS_FILE.tmp.$$"
  if printf '{"started_at":"%s","finished_at":%s,"exit_code":%s,"step":"%s","new_files":%s}\n' \
    "$STARTED_AT" "$fin" "$code" "$STEP" "$NEW_FILES" >"$tmp" 2>/dev/null &&
    mv -f "$tmp" "$STATUS_FILE" 2>/dev/null; then
    return 0
  fi
  rm -f "$tmp" 2>/dev/null || true
  return 1
}

acquire_lock() { # 0 acquired, 1 held by a live run, 2 cannot create
  local pid=""
  if mkdir "$LOCKDIR" 2>/dev/null; then
    echo $$ >"$LOCKDIR/pid"
    return 0
  fi
  [ -d "$LOCKDIR" ] || return 2
  pid="$(cat "$LOCKDIR/pid" 2>/dev/null || true)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null &&
    [ -z "$(find "$LOCKDIR" -maxdepth 0 -mmin +720 2>/dev/null)" ]; then
    return 1
  fi
  # No pid yet: the holder is between mkdir and its pid write; a young lock is live.
  if [ -z "$pid" ] && [ -n "$(find "$LOCKDIR" -maxdepth 0 -mmin -1 2>/dev/null)" ]; then
    return 1
  fi
  rm -rf "$LOCKDIR"
  mkdir "$LOCKDIR" 2>/dev/null || return 1
  echo $$ >"$LOCKDIR/pid"
}

if ! mkdir -p "$(dirname "$STATUS_FILE")" 2>/dev/null; then
  echo "scraper: cannot create $(dirname "$STATUS_FILE"); run not recorded, not started" >&2
  exit 74
fi
rc=0
acquire_lock || rc=$?
case "$rc" in
0) ;;
1)
  echo "scraper: another run holds $LOCKDIR; exiting without touching the status" >&2
  exit 75 # EX_TEMPFAIL
  ;;
*)
  echo "scraper: cannot create lock $LOCKDIR; run not recorded, not started" >&2
  exit 74
  ;;
esac

release_lock() {
  rm -f "$LOCKDIR/pid" 2>/dev/null || true
  # If the dir cannot be removed (read-only parent), mark it released so the next run replaces it.
  rmdir "$LOCKDIR" 2>/dev/null || echo released >"$LOCKDIR/pid" 2>/dev/null || true
}
on_exit() {
  local code=$?
  trap - EXIT
  if ! write_status "$code" "$(now)"; then
    echo "scraper: could not write $STATUS_FILE; this run is NOT recorded (exit $code)" >&2
    [ "$code" -ne 0 ] || code=74 # EX_IOERR
  fi
  release_lock
  exit "$code"
}
on_signal() { # exit code
  trap '' TERM HUP INT
  if [ -n "$CHILD" ]; then
    kill -TERM "$CHILD" 2>/dev/null || true
    wait "$CHILD" 2>/dev/null || true
  fi
  exit "$1"
}
trap on_exit EXIT
trap 'on_signal 143' TERM
trap 'on_signal 129' HUP
trap 'on_signal 130' INT

set_step() { # persisted, so a SIGKILL shows the step it died in
  STEP="$1"
  write_status null null || true
}
# Child in the background and `wait`: a signal reaches the trap now, not after the scrape ends.
run_step() {
  "$@" &
  CHILD=$!
  local rc=0
  wait "$CHILD" || rc=$?
  CHILD=""
  return "$rc"
}
count_files() { find "$1" -type f 2>/dev/null | wc -l | tr -d ' ' || true; }

if ! write_status null null; then
  echo "scraper: could not write $STATUS_FILE; run not recorded, not started" >&2
  exit 74
fi

if [ ! -f "$WATCHLIST" ]; then
  echo "scraper: watchlist not written yet: $WATCHLIST (start finance-app first)" >&2
  exit 78 # EX_CONFIG
fi
if [ ! -f "$FINANCE_IDX_SCRAPER_DIR/run.py" ]; then
  echo "scraper: run.py not found in $FINANCE_IDX_SCRAPER_DIR" >&2
  exit 78
fi

cd "$FINANCE_IDX_SCRAPER_DIR"
BEFORE="$(count_files "$XBRL_DIR")"
set_step download
run_step "$PYTHON" run.py --skip-master --watchlist "$WATCHLIST"
AFTER="$(count_files "$XBRL_DIR")"
NEW_FILES=$((AFTER > BEFORE ? AFTER - BEFORE : 0))
set_step export
run_step "$PYTHON" model/sqlite_export.py
set_step done
