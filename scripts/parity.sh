#!/usr/bin/env bash
# API parity (prd.md S1, A1; plan.md §3.1b): the same GETs on a second finance-app
# instance (A) and on investing-app (B), each on its own copy of one snapshot (D2),
# diffed as JSON.
#
#   A_URL=... A_TOKEN=... B_URL=... B_TOKEN=... scripts/parity.sh
#   TICKERS="BUMI BRIS BBCA" ...                  # default: A's held tickers + first peer
#
# Env (required, no defaults): A_URL A_TOKEN B_URL B_TOKEN. A is the finance-app side.
# Output: `ok <path>` or `DIFF <path>` followed by the first 40 lines of the diff.
# A different HTTP status on the two sides is a DIFF. `SKIP` (an id that A's list
# responses do not offer) never fails the run.
# Exit: 0 nothing differs, 1 something differs, 2 usage error or a refused URL.
#
# NUMBER RULE (finance-app scripts/parity.sh:28, owner decision): compare values, not
# representation. jq 1.7 preserves number literals, so 13000.0 and 13000 would diff as
# text; every integral number becomes an integer and real fractions stay untouched.
set -uo pipefail

die() {
  echo "parity: $*" >&2
  exit 2
}

for v in A_URL A_TOKEN B_URL B_TOKEN; do
  [ -n "${!v:-}" ] || die "set $v (A_URL A_TOKEN B_URL B_TOKEN are required, no defaults)"
done
A_URL="${A_URL%/}"
B_URL="${B_URL%/}"

# Live finance-app is never a parity target (D2). Checked before any request.
for u in "$A_URL" "$B_URL"; do
  if [[ "$u" =~ :8787([^0-9]|$) ]]; then
    die "refusing $u: port 8787 is the live finance-app"
  fi
done

for tool in curl jq diff; do
  command -v "$tool" >/dev/null 2>&1 || die "$tool not found"
done

NUM='walk(if type == "number" then (if . == floor then floor else . end) else . end)'

# Named drops: fields that legitimately differ. Anything else that differs is a bug.
# Each filter is applied to both sides, after NUM, to objects only (error bodies of
# other shapes pass through unchanged). Reasons are for finance-app at 4ef2dd9.
# (case statements, not associative arrays: macOS ships bash 3.2)
drop_for() {
  case "$1" in
    # Process load time; absolute packs directory of each checkout
    # (src/research/api.rs:895-905, src/research/packs/builtin.rs:137-148).
    /api/research/packs)
      echo 'del(.status.loaded_at, .status.source, .status.dir)' ;;
    # Same: process load time and checkout path (src/research/valuation.rs:121-139).
    /api/research/rules)
      echo 'del(.loaded_at, .source)' ;;
    # NEW (not in plan.md's table; owner OK needed in the L1b PR): request time
    # (src/research/events/react.rs:442).
    /api/research/desk)
      echo 'del(.as_of)' ;;
    # NEW (same): request time (src/research/api/r4.rs:1053).
    /api/research/calibration)
      echo 'del(.as_of)' ;;
    # Request time (src/research/api.rs:963, src/research/api/lookup.rs:110);
    # NEW: .items[].added_at and .items[].synced_at are the POST /universe write time when
    # self-test step 5 adds a ticker to each running instance (src/research/api.rs:673-674,1142).
    /api/research/universe)
      echo 'del(.as_of, .items[]?.added_at, .items[]?.synced_at)' ;;
    /api/research/lookup\?q=*)
      echo 'del(.as_of)' ;;
    /api/research/compare | /api/research/filings)
      echo '' ;;
    # Request time (finance-app scripts/parity.sh:68).
    /api/finance/stock-portfolio)
      echo 'del(.summary.updated_at)' ;;
    # Request time (finance-app scripts/parity.sh:69). /api/thesis/{T} carries no
    # generated_at (checked on the vault fixture), so it has no drop.
    /api/thesis/summary | /api/thesis/macro)
      echo 'del(.generated_at)' ;;
    # Request time (finance-app scripts/parity.sh:70).
    /api/macro/observations)
      echo 'del(.as_of)' ;;
    # Request time (finance-app scripts/parity.sh:71).
    /api/macro/scoring)
      echo 'del(.as_of, .score_vintage)' ;;
    *)
      # Per-ticker paths. A case glob's `*` also matches `/`, so these are regexes with one
      # segment per `[^/?#]+`: every other /api/research path keeps its as_of.
      # Request time (src/research/valuation.rs:882 for /{T}/valuation).
      # NEW (not in plan.md's table; owner OK needed in the L1b PR): request time
      # (src/research/api/r5.rs:129 for /{T}, src/research/packs/payload.rs:705 for /pack,
      # src/research/prices/payload.rs:403 for /prices).
      if [[ "$1" =~ ^/api/research/[^/?#]+(/(valuation|pack|prices))?$ ]]; then
        echo 'del(.as_of)'
      else
        echo ''
      fi ;;
  esac
}

TMP="$(umask 077 && mktemp -d)" || die "mktemp failed"
trap 'rm -rf "$TMP"' EXIT
printf 'Authorization: Bearer %s\n' "$A_TOKEN" >"$TMP/auth.a"
printf 'Authorization: Bearer %s\n' "$B_TOKEN" >"$TMP/auth.b"

# fetch <a|b> <path> <body-file> -> prints the HTTP status ("000" when unreachable).
fetch() {
  local base="$A_URL"
  [ "$1" = b ] && base="$B_URL"
  curl -s -m 60 -H "@$TMP/auth.$1" -o "$3" -w '%{http_code}' "$base$2" 2>/dev/null || true
}

# normalize <path> <in> <out>: jq -S with NUM and the path's drop; a body that is not
# JSON is compared as text.
normalize() {
  local drop filter
  drop="$(drop_for "$1")"
  filter="$NUM"
  [ -n "$drop" ] && filter="$NUM | if type == \"object\" then $drop else . end"
  jq -S "$filter" "$2" >"$3" 2>/dev/null || cp "$2" "$3"
}

fail=0
total=0

report_diff() { # <path> <header line> <file-a> <file-b>
  echo "DIFF $1"
  [ -n "$2" ] && echo "  $2"
  if [ -n "$3" ]; then
    diff "$3" "$4" | head -n 40
  fi
  fail=$((fail + 1))
}

check() { # <path>
  local p="$1" sa sb
  total=$((total + 1))
  sa="$(fetch a "$p" "$TMP/a.raw")"
  sb="$(fetch b "$p" "$TMP/b.raw")"
  if [ "$sa" != "$sb" ] || [ "$sa" = 000 ]; then
    report_diff "$p" "status A=$sa B=$sb" "" ""
    return
  fi
  normalize "$p" "$TMP/a.raw" "$TMP/a.json"
  normalize "$p" "$TMP/b.raw" "$TMP/b.json"
  if diff -q "$TMP/a.json" "$TMP/b.json" >/dev/null; then
    echo "ok $p"
  else
    report_diff "$p" "" "$TMP/a.json" "$TMP/b.json"
  fi
}

# id_from <path> <jq expr>: first id from A's response of <path>; the expression prints
# nothing (not "null") when there is none.
id_from() {
  local st
  st="$(fetch a "$1" "$TMP/id.raw")"
  [ "$st" = 200 ] || return 0
  jq -r "$2" "$TMP/id.raw" 2>/dev/null || true
}

check_id() { # <path template for messages> <path> (empty path = no id in A)
  if [ -z "$2" ]; then
    echo "SKIP $1 (no id in A's list response)"
  else
    check "$2"
  fi
}

# Tickers: TICKERS, else every held ticker in A's universe plus the first peer.
if [ -z "${TICKERS:-}" ]; then
  st="$(fetch a /api/research/universe "$TMP/universe.json")"
  [ "$st" = 200 ] || die "A /api/research/universe returned $st; cannot pick tickers (set TICKERS)"
  TICKERS="$(jq -r '([.items[] | select(.role == "held") | .ticker]
                     + [first(.items[] | select(.role == "peer") | .ticker)?]) | .[]' \
    "$TMP/universe.json" 2>/dev/null | tr '\n' ' ')"
fi
# shellcheck disable=SC2206  # word splitting of the ticker list is intended
TICKER_LIST=($TICKERS)
[ "${#TICKER_LIST[@]}" -gt 0 ] || die "no tickers (A has no held ticker; set TICKERS)"

# prd.md §8 A1, in order.
for p in /api/research/desk /api/research/compare /api/research/calibration \
  /api/research/packs /api/research/rules /api/research/filings /api/research/universe; do
  check "$p"
done
for t in "${TICKER_LIST[@]}"; do
  check "/api/research/lookup?q=$t"
done

for t in "${TICKER_LIST[@]}"; do
  check "/api/research/$t"
  for view in standalone ttm annual; do
    check "/api/research/$t/financials?view=$view"
  done
  for s in pack valuation prices structure claims journal proposals calendar documents; do
    check "/api/research/$t/$s"
  done
done

# By id, taken from A's list responses in this run.
for t in "${TICKER_LIST[@]}"; do
  # GET /facts/{filing_id}/{element} needs ?ctx= (and takes dims=); a fact ref in the financials
  # response carries both. The annual view holds fact refs only when a ticker has an FY filing
  # (the fixture's AAAA has only Q2), so the standalone view is the fallback.
  fact=
  for view in annual standalone; do
    [ -n "$fact" ] || fact="$(id_from "/api/research/$t/financials?view=$view&periods=4" \
      'first(.. | objects | select(.filing_id and .element)) | "\(.filing_id | @uri)/\(.element | @uri)?ctx=\((.ctx // "") | @uri)&dims=\((.dims // "") | @uri)"')"
  done
  check_id "/api/research/facts/{filing_id}/{element} ($t)" \
    "${fact:+/api/research/facts/$fact}"
  claim="$(id_from "/api/research/$t/claims" '(.items[0].id // empty) | @uri')"
  check_id "/api/research/claims/{id}/evidence ($t)" "${claim:+/api/research/claims/$claim/evidence}"
  check_id "/api/research/claims/{id}/evals ($t)" "${claim:+/api/research/claims/$claim/evals}"
  entry="$(id_from "/api/research/$t/journal" '(.items[0].id // empty) | @uri')"
  check_id "/api/research/journal/{id} ($t)" "${entry:+/api/research/journal/$entry}"
done
draft="$(id_from /api/thesis/drafts '(.drafts[0].id // empty) | @uri')"
check_id "/api/thesis/drafts/{id}" "${draft:+/api/thesis/drafts/$draft}"

check /api/finance/stock-portfolio
check /api/thesis/summary
check /api/thesis/macro
for t in "${TICKER_LIST[@]}"; do
  check "/api/thesis/$t"
done
check /api/thesis/drafts
check /api/thesis/drafts/vault
check /api/macro/observations
check /api/macro/registry
check /api/macro/scoring

# /api/macro/stream is an event stream: status and content-type only (no screen reads it, D12).
stream() { # <a|b> -> "<status> <content-type>"
  local base="$A_URL" st ct
  [ "$1" = b ] && base="$B_URL"
  st="$(curl -s -m 3 -H "@$TMP/auth.$1" -D "$TMP/stream.$1" -o /dev/null -w '%{http_code}' \
    "$base/api/macro/stream" 2>/dev/null || true)"
  ct="$(tr -d '\r' <"$TMP/stream.$1" 2>/dev/null |
    awk 'tolower($1) == "content-type:" { $1 = ""; sub(/^ /, ""); print tolower($0) }' | head -n 1)"
  echo "$st $ct"
}
total=$((total + 1))
sta="$(stream a)"
stb="$(stream b)"
if [ "$sta" = "$stb" ] && [[ "$sta" == "200 text/event-stream"* ]]; then
  echo "ok /api/macro/stream"
else
  report_diff /api/macro/stream "A='$sta' B='$stb' (want '200 text/event-stream' on both)" "" ""
fi

echo "parity: $fail of $total paths differ"
[ "$fail" -eq 0 ]
