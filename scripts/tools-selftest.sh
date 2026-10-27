#!/usr/bin/env bash
# scripts/tools-selftest.sh (plan.md §3.1b, steps 1-10): proves counts.sh, parity.sh and writes.sh
# on fixtures. No live data: two binaries on 127.0.0.1:18801 (A) and :18802 (B, sandbox), each with
# a scratch HOME, a mktemp data dir and a throwaway token. No network beyond what macro/trigger does
# with MACRO_DISABLE_FRED=1. Binaries are stopped by PID, never by name (other worktrees run the
# same binary). Exit 0 only when every step matched, 1 otherwise.
# No -e on purpose: the scripts under test exit non-zero in the negative steps (7, 9), and each
# step records its own pass or fail instead of aborting the run.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

PORT_A=18801
PORT_B=18802
A_URL="http://127.0.0.1:$PORT_A"
B_URL="http://127.0.0.1:$PORT_B"
BIN=./target/release/investing-app

failed=0
step() { printf '\n== step %s: %s\n' "$1" "$2"; }
pass() { echo "PASS step $1: $2"; }
fail() {
  echo "FAIL step $1: $2"
  failed=$((failed + 1))
}

for tool in cargo curl jq openssl rsync sqlite3; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "tools-selftest: $tool not found" >&2
    exit 2
  }
done

# The §1 snapshot block, verbatim, so the self-test exercises it too.
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

PID_A=
PID_B=
W=
# stop_pid PID: TERM, then KILL after 5 s; only the PID this script started.
stop_pid() {
  [ -n "$1" ] || return 0
  kill "$1" 2>/dev/null || return 0
  for _ in $(seq 20); do
    kill -0 "$1" 2>/dev/null || break
    sleep 0.25
  done
  kill -9 "$1" 2>/dev/null || true
  wait "$1" 2>/dev/null || true
}
cleanup() {
  stop_pid "$PID_A"
  stop_pid "$PID_B"
  [ -z "$W" ] || rm -rf "$W"
}
trap cleanup EXIT
trap 'exit 1' INT TERM

up() { curl -s -m 1 "$1/api/health" >/dev/null 2>&1; }

# start SIDE PORT DIR SANDBOX(0|1): the §3.1 smoke env, scratch HOME per side; sets PID_<SIDE>.
start() {
  local side=$1 port=$2 dir=$3 sandbox=$4 url="http://127.0.0.1:$2" tok pid
  if up "$url"; then
    echo "port $port is already in use"
    return 1
  fi
  if [ "$side" = A ]; then tok=$TOKEN_A; else tok=$TOKEN_B; fi
  mkdir -p "$W/home-$side"
  if [ "$sandbox" = 1 ]; then set -- INVESTING_SANDBOX=1; else set --; fi
  env -i PATH="$PATH" HOME="$W/home-$side" \
    INVESTING_APP_TOKEN="$tok" INVESTING_BIND="127.0.0.1:$port" INVESTING_DATA_DIR="$dir" \
    PORTFOLIO_SPREADSHEET_ID=smoke GOOGLE_APPLICATION_CREDENTIALS="$W/home-$side/none.json" MACRO_DISABLE_FRED=1 \
    INVESTING_RESEARCH_DISABLE_SCHEDULER=1 INVESTING_MACRO_WATCHER=0 "$@" \
    "$BIN" >>"$W/$side.log" 2>&1 &
  pid=$!
  if [ "$side" = A ]; then PID_A=$pid; else PID_B=$pid; fi
  for _ in $(seq 120); do
    up "$url" && break
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  curl -s -m 2 "$url/api/health"
  echo
  up "$url"
}

# post SIDE KEY PATH JSON: an authorised write with Idempotency-Key; prints the status.
post() {
  local url=$A_URL auth=$W/auth.A
  if [ "$1" = B ]; then url=$B_URL auth=$W/auth.B; fi
  curl -s -m 30 -o "$W/post.json" -w '%{http_code}' -X POST -H "@$auth" \
    -H "Idempotency-Key: selftest-$$-$2" -H 'Content-Type: application/json' \
    --data-binary "$4" "$url$3" || true
}

lines() { wc -l <"$1" | tr -d ' '; }

# seed_f1 DIR: the rows behind parity.sh's by-id paths, written into F1 before the snapshot so F2
# inherits them byte for byte. Two API seeds could not match: a claim's created_at and eval
# evaluated_at, a journal entry's at and a draft's id and created_at come from each server's own
# clock (src/research/api/r4.rs:234,471, src/thesis/mod.rs:216-219; the clock is injected only by
# tests, src/research/engine.rs). Row shapes are what the API wrote on a scratch run.
SEED_CLAIM=c-seed1
SEED_JOURNAL=j-seed1
SEED_DRAFT=draft_aaaa_1790000000_seed01
seed_f1() {
  local dir="$1" at=2026-10-27T00:00:00+00:00
  sqlite3 -bail "$dir/research.sqlite" <<SQL || return 1
INSERT INTO claims VALUES ('$SEED_CLAIM', 'AAAA', 'Ekuitas positif', 'core.total_equity', '>', 0.0, NULL, 'latest', '2027-03-31', 'holding', '$at', NULL);
INSERT INTO claim_evals VALUES ('$SEED_CLAIM', '$at', 'holding', 260100000.0, '{"window":"latest","as_of":"2026-10-27","observations":[],"reason":null}');
INSERT INTO documents VALUES ('d-seed1', 'AAAA', 'artikel', 'https://example.test/berita', NULL, NULL, '$at');
INSERT INTO evidence VALUES ('$SEED_CLAIM', NULL, 'd-seed1');
INSERT INTO journal VALUES ('$SEED_JOURNAL', 'AAAA', '$at', 'keputusan', 'Menambah posisi', NULL, NULL);
SQL
  cat >"$dir/thesis_drafts.json" <<JSON
{"$SEED_DRAFT": {"id": "$SEED_DRAFT", "ticker": "AAAA", "title": "SEED", "thesis_statement": "x",
  "thesis_type": "fundamental", "is_primary": false, "version": 1, "status": "draft",
  "scenario_status": "Menunggu", "story_status": "Belum terkonfirmasi", "falsifiers": ["y"],
  "triggers_add": [], "triggers_exit": [], "notes": "", "created_at": "$at", "updated_at": "$at"}}
JSON
}

# 1. Release build and a scratch work dir.
step 1 "cargo build --release --locked; W=\$(mktemp -d)"
if cargo build --release --locked; then
  pass 1 "release build"
else
  fail 1 "cargo build --release --locked"
  echo "tools-selftest: cannot continue without the binary"
  exit 1
fi
W="$(mktemp -d)"
TOKEN_A="$(openssl rand -hex 32)"
TOKEN_B="$(openssl rand -hex 32)"
(
  umask 077
  printf 'Authorization: Bearer %s\n' "$TOKEN_A" >"$W/auth.A"
  printf 'Authorization: Bearer %s\n' "$TOKEN_B" >"$W/auth.B"
)
echo "W=$W"

# 2. Fixture dir F1.
step 2 "fixture dir F1"
mkdir -p "$W/f1/research/"{documents,structure,pdf-filings} "$W/home-A"
if env -i PATH="$PATH" HOME="$W/home-A" INVESTING_DATA_DIR="$W/f1" \
  "$BIN" research-ingest --xbrl-dir tests/fixtures/research/xbrl --db "$W/f1/research.sqlite" &&
  cp tests/fixtures/macro/store.json "$W/f1/macro_store.json" &&
  : >"$W/f1/research-log.jsonl" && : >"$W/f1/research-idempotency.jsonl" &&
  seed_f1 "$W/f1"; then
  tickers="$(sqlite3 -readonly "file:$W/f1/research.sqlite?mode=ro&immutable=1" 'SELECT group_concat(ticker, " ") FROM (SELECT DISTINCT ticker FROM filings ORDER BY ticker)')"
  echo "filings tickers: $tickers"
  if [ "$tickers" = "AAAA BBBB CCCC" ]; then pass 2 "F1 holds AAAA BBBB CCCC"; else fail 2 "F1 tickers: $tickers"; fi
else
  fail 2 "building F1"
fi

# 3. F2 = snapshot of F1.
step 3 "snapshot \"\$W/f1\" \"\$W/f2\""
if snapshot "$W/f1" "$W/f2"; then
  if snapshot "$W/f1" "$W/f2" 2>/dev/null; then
    fail 3 "a second snapshot into an existing target did not refuse"
  else
    pass 3 "F2 written; a second snapshot into it refuses"
  fi
else
  fail 3 "snapshot"
fi

# 4. counts on two identical dirs.
step 4 "scripts/counts.sh \"\$W/f1\" \"\$W/f2\""
out="$(scripts/counts.sh "$W/f1" "$W/f2")"
rc=$?
echo "$out"
echo "exit=$rc"
# Negative: a malformed drafts file on one side is exit 2, not a 0 count (review L1b #2).
cp -R "$W/f2" "$W/f2-bad"
echo '[' >"$W/f2-bad/thesis_drafts.json"
scripts/counts.sh "$W/f1" "$W/f2-bad"
crc=$?
echo "malformed drafts on B: exit=$crc"
rm -rf "$W/f2-bad"
if [ "$rc" = 0 ] && [ "$(printf '%s\n' "$out" | tail -n 1)" = "counts: 0 differences" ] && [ "$crc" = 2 ]; then
  pass 4 "exit 0, counts: 0 differences; a malformed drafts file exits 2"
else
  fail 4 "expected exit 0 and 'counts: 0 differences', and exit 2 on a malformed drafts file"
fi

# 5. A on F1, B on F2 (sandbox); AAAA as a manual ticker on both (research_universe_api.rs:
# POST /api/research/universe with an Idempotency-Key, role defaults to manual).
step 5 "start A on F1 at :$PORT_A, B on F2 at :$PORT_B (INVESTING_SANDBOX=1); add AAAA"
servers=1
start A "$PORT_A" "$W/f1" 0 || servers=0
start B "$PORT_B" "$W/f2" 1 || servers=0
if [ "$servers" = 1 ]; then
  u_body='{"ticker": "AAAA", "pack_override": "it_distribution", "business_note": "Distributor sintetis"}'
  sa="$(post A u1 /api/research/universe "$u_body")"
  sb="$(post B u1 /api/research/universe "$u_body")"
  echo "add AAAA: A=$sa B=$sb"
  if curl -s -m 2 "$B_URL/api/health" | jq -e '.sandbox == true' >/dev/null &&
    ! curl -s -m 2 "$A_URL/api/health" | jq -e '.sandbox == true' >/dev/null &&
    [ "$sa" = 201 ] && [ "$sb" = 201 ]; then
    pass 5 "both up, only B is the sandbox, AAAA added 201/201"
  else
    fail 5 "health or AAAA add"
  fi
else
  fail 5 "a binary did not come up"
  tail -n 20 "$W/A.log" "$W/B.log"
  echo "tools-selftest: steps 6-9 need both binaries"
  exit 1
fi

export A_URL B_URL A_TOKEN="$TOKEN_A" B_TOKEN="$TOKEN_B"

# 6. parity on identical data.
step 6 "TICKERS=AAAA scripts/parity.sh"
out="$(TICKERS=AAAA scripts/parity.sh)"
rc=$?
echo "$out"
echo "exit=$rc"
# The five by-id paths must run on the seeded ids: ok, never SKIP (review L1b #4).
byid_ok=1
for pat in "^ok /api/research/facts/AAAA_2025_Q2/[^ ]*ctx=" \
  "^ok /api/research/claims/$SEED_CLAIM/evidence\$" "^ok /api/research/claims/$SEED_CLAIM/evals\$" \
  "^ok /api/research/journal/$SEED_JOURNAL\$" "^ok /api/thesis/drafts/$SEED_DRAFT\$"; do
  printf '%s\n' "$out" | grep -q "$pat" || { echo "by-id path missing an ok line: $pat"; byid_ok=0; }
done
if [ "$rc" = 0 ] && ! printf '%s\n' "$out" | grep -q '^DIFF' && ! printf '%s\n' "$out" | grep -q '^SKIP' &&
  [ "$byid_ok" = 1 ]; then
  pass 6 "exit 0, no DIFF, no SKIP; the five by-id paths (facts, claims evidence/evals, journal, drafts) ok"
else
  fail 6 "expected exit 0, no DIFF or SKIP, and ok on the five by-id paths"
fi

# 7. One extra claim on B only (research_claims.rs body): parity and counts must both see it.
step 7 "negative: one extra claim on B only"
s="$(post B c1 /api/research/AAAA/claims '{"statement": "Ekuitas positif", "metric": "core.total_equity", "comparator": ">", "threshold": 0.0, "window": "latest", "due": "2027-03-31"}')"
echo "extra claim on B: $s"
out="$(TICKERS=AAAA scripts/parity.sh)"
prc=$?
printf '%s\n' "$out" | grep -E '^(DIFF|parity:)'
echo "parity exit=$prc"
cout="$(scripts/counts.sh "$W/f1" "$W/f2")"
crc=$?
echo "$cout"
echo "counts exit=$crc"
tab="$(printf '\t')"
if [ "$s" = 201 ] && [ "$prc" = 1 ] && printf '%s\n' "$out" | grep -qx 'DIFF /api/research/AAAA/claims' &&
  [ "$crc" = 1 ] && printf '%s\n' "$cout" | grep -q "^claims${tab}AAAA${tab}"; then
  pass 7 "parity exit 1 naming /api/research/AAAA/claims; counts exit 1 naming claims AAAA"
else
  fail 7 "expected claim 201, parity exit 1 with DIFF /api/research/AAAA/claims, counts exit 1 with claims AAAA"
fi

# 8. writes: every pair matches; refresh 503/503; structure draft ok; SKIP only on steps 12, 13.
step 8 "TICKER=AAAA scripts/writes.sh"
out="$(A_DATA="$W/f1" B_DATA="$W/f2" TICKER=AAAA scripts/writes.sh)"
rc=$?
echo "$out"
echo "exit=$rc"
skips="$(printf '%s\n' "$out" | grep "${tab}SKIP" || true)"
bad_skips="$(printf '%s\n' "$skips" | grep -vE '^1[23] ' | grep . || true)"
if [ -n "$skips" ]; then
  echo "SKIP lines:"
  printf '%s\n' "$skips"
else
  echo "SKIP lines: none"
fi
if [ "$rc" = 0 ] &&
  printf '%s\n' "$out" | grep -qx "22 POST /api/research/refresh?ticker=AAAA${tab}503${tab}503${tab}ok" &&
  printf '%s\n' "$out" | grep -qE "^14 POST /api/research/AAAA/structure/draft${tab}[0-9]+${tab}[0-9]+${tab}ok\$" &&
  [ -z "$bad_skips" ]; then
  pass 8 "exit 0, refresh 503 503 ok, structure draft ok, SKIP only on 12/13"
else
  fail 8 "expected exit 0, refresh 503 503 ok, structure draft ok, SKIP only on 12/13"
fi

# 9. Guards. A curl shim on PATH records any request, so "before any request" is observed and no
# request can reach :8787 even if a guard were broken.
step 9 "guards: :8787 refused by parity.sh and writes.sh; writes.sh refuses a B without INVESTING_SANDBOX"
mkdir -p "$W/shim"
printf '#!/bin/sh\necho "$*" >>"%s"\nexit 7\n' "$W/curl-calls" >"$W/shim/curl"
chmod +x "$W/shim/curl"
ok9=1
PATH="$W/shim:$PATH" A_URL=http://127.0.0.1:8787 scripts/parity.sh
rc=$?
echo "parity.sh A_URL=:8787 exit=$rc, curl calls: $(cat "$W/curl-calls" 2>/dev/null | wc -l | tr -d ' ')"
{ [ "$rc" = 2 ] && [ ! -e "$W/curl-calls" ]; } || ok9=0
PATH="$W/shim:$PATH" A_URL=http://127.0.0.1:8787 A_DATA="$W/f1" B_DATA="$W/f2" TICKER=AAAA scripts/writes.sh
rc=$?
echo "writes.sh A_URL=:8787 exit=$rc, curl calls: $(cat "$W/curl-calls" 2>/dev/null | wc -l | tr -d ' ')"
{ [ "$rc" = 2 ] && [ ! -e "$W/curl-calls" ]; } || ok9=0
# A data dir that is, or links to, $HOME/finance-app-data (a fake HOME here) is refused before
# any request or write (review L1b #6); nothing is created inside it.
mkdir -p "$W/fakehome/finance-app-data"
ln -s "$W/fakehome/finance-app-data" "$W/live-link"
for live in "$W/fakehome/finance-app-data" "$W/live-link"; do
  PATH="$W/shim:$PATH" HOME="$W/fakehome" A_DATA="$live" B_DATA="$W/f2" TICKER=AAAA scripts/writes.sh
  rc=$?
  echo "writes.sh A_DATA=${live#"$W"/}: exit=$rc, curl calls: $(cat "$W/curl-calls" 2>/dev/null | wc -l | tr -d ' ')"
  { [ "$rc" = 2 ] && [ ! -e "$W/curl-calls" ] && [ -z "$(ls -A "$W/fakehome/finance-app-data")" ]; } || ok9=0
done
# B restarted on F2 without INVESTING_SANDBOX: refused, and neither side's log grows.
stop_pid "$PID_B"
PID_B=
if start B "$PORT_B" "$W/f2" 0; then
  before="$(lines "$W/f1/research-log.jsonl") $(lines "$W/f2/research-log.jsonl")"
  A_DATA="$W/f1" B_DATA="$W/f2" TICKER=AAAA scripts/writes.sh
  rc=$?
  after="$(lines "$W/f1/research-log.jsonl") $(lines "$W/f2/research-log.jsonl")"
  echo "writes.sh, B without sandbox: exit=$rc; research-log lines A B before: $before, after: $after"
  { [ "$rc" = 2 ] && [ "$before" = "$after" ]; } || ok9=0
else
  echo "B did not restart without the sandbox flag"
  ok9=0
fi
if [ "$ok9" = 1 ]; then
  pass 9 "all three guards exit 2 before any request or write"
else
  fail 9 "a guard did not exit 2 before any request or write"
fi

# 10. Stop both binaries by PID.
step 10 "stop both binaries"
stop_pid "$PID_A"
stop_pid "$PID_B"
PID_A=
PID_B=
if ! up "$A_URL" && ! up "$B_URL"; then
  pass 10 "both stopped (:$PORT_A and :$PORT_B refuse)"
else
  fail 10 "a binary still answers"
fi

echo
if [ "$failed" -eq 0 ]; then
  echo "tools-selftest: all 10 steps passed"
else
  echo "tools-selftest: $failed step(s) failed"
fi
[ "$failed" -eq 0 ]
