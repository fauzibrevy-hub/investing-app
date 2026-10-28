#!/usr/bin/env bash
# scripts/writes.sh (plan.md §3.1b, prd.md A3): every A3 write, sent to A and then to B, compared by status.
# Env (all required): A_URL A_TOKEN A_DATA B_URL B_TOKEN B_DATA TICKER. A = the second finance-app instance,
# B = the investing-app sandbox; *_DATA is each side's data dir (step 14 writes its draft file there).
# Prints label<TAB>a<TAB>b<TAB>ok|DIFF|SKIP <reason>. Exit 0 when every pair matches, 1 otherwise,
# 2 on usage, a refused URL (:8787), an A_DATA or B_DATA that is (or links to) a live data dir under
# $HOME, or a B whose /api/health lacks "sandbox": true, all before any write.
# Tokens are never printed and never on a command line: curl reads them from a mode-600 header file.
# Runs on macOS /bin/bash 3.2.
set -euo pipefail
cd "$(dirname "$0")/.."

usage() {
  echo "writes.sh: $*" >&2
  exit 2
}

for v in A_URL A_TOKEN A_DATA B_URL B_TOKEN B_DATA TICKER; do
  [ -n "${!v:-}" ] || usage "$v is required (env: A_URL A_TOKEN A_DATA B_URL B_TOKEN B_DATA TICKER)"
done
# D2: the live finance-app port is never a target.
# LIVE_PORT_RE is the same line as in parity.sh: any host, then :8787 as the whole port.
LIVE_PORT_RE='^([A-Za-z][A-Za-z0-9+.-]*://)?[^/?#]*:8787([/?#]|$)'
for v in A_URL B_URL; do
  if [[ "${!v}" =~ $LIVE_PORT_RE ]]; then
    usage "$v=${!v} refused: :8787 is the live finance-app"
  fi
done
A_URL=${A_URL%/}
B_URL=${B_URL%/}
TICKER=$(printf '%s' "$TICKER" | tr '[:lower:]' '[:upper:]')
[[ $TICKER =~ ^[A-Z0-9]{2,10}$ ]] || usage "TICKER=$TICKER is not a ticker"
for d in "$A_DATA" "$B_DATA"; do
  [ -d "$d" ] || usage "data dir $d does not exist"
done
# Live data dirs are never a write target (CLAUDE.md live-data rule): step 14 writes into *_DATA.
# Symlinks are resolved, so a scratch-looking path that points at a live dir is refused too.
if [ -n "${HOME:-}" ] && [ -d "$HOME" ]; then
  home=$(cd "$HOME" && pwd -P)
  for d in "$A_DATA" "$B_DATA"; do
    real=$(cd "$d" && pwd -P)
    case "$real" in
      "$home/finance-app-data" | "$home/finance-app-data/"* | "$home/investing-app-data" | "$home/investing-app-data/"*)
        usage "data dir $d resolves to $real: a live data dir, refused before any write" ;;
    esac
  done
fi
[ -f tests/fixtures/research/structure-BUMI.draft.json ] || usage "tests/fixtures/research/structure-BUMI.draft.json missing"

# D7: B must be the sandbox, checked before anything is written.
if ! curl -s -m 5 "$B_URL/api/health" | jq -e '.sandbox == true' >/dev/null 2>&1; then
  usage "B_URL=$B_URL: /api/health has no \"sandbox\": true (start B with INVESTING_SANDBOX=1)"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
(
  umask 077
  printf 'Authorization: Bearer %s\n' "$A_TOKEN" >"$tmp/a.auth"
  printf 'Authorization: Bearer %s\n' "$B_TOKEN" >"$tmp/b.auth"
)

# One Idempotency-Key per request pair, fresh per run, so a rerun on the same data never replays.
run_id="writes-$(date +%s)-$$"
n=0
diffs=0
code=
sa=-
sb=-

url_of() { if [ "$1" = a ]; then echo "$A_URL"; else echo "$B_URL"; fi; }

# send SIDE METHOD PATH [JSON | --form]: status into $code, body into $tmp/SIDE.json.
# --form sends the step-13 upload (finance-app tests/research_extract.rs FORM + coal_pdf()).
send() {
  local side=$1 method=$2 path=$3 body=${4:-} url
  url=$(url_of "$side")
  set -- -s -m "${WRITES_TIMEOUT:-60}" -o "$tmp/$side.json" -w '%{http_code}' -X "$method" \
    -H "@$tmp/$side.auth" -H "Idempotency-Key: $run_id-$n"
  if [ "$body" = --form ]; then
    set -- "$@" -F kind=presentasi -F doc_date=2026-08-20 -F fy=2026 -F q=2 \
      -F "file=@$tmp/paparan-q2.pdf;filename=paparan-q2.pdf;type=application/octet-stream"
  elif [ -n "$body" ]; then
    set -- "$@" -H 'Content-Type: application/json' --data-binary "$body"
  fi
  code=$(curl "$@" "$url$path") || true
  [ -n "$code" ] || code=000
}

# get SIDE PATH: a read into $tmp/SIDE.get.json, for the ids steps 12, 13 and 15 take from the data.
get() {
  curl -s -m "${WRITES_TIMEOUT:-60}" -o "$tmp/$1.get.json" -H "@$tmp/$1.auth" "$(url_of "$1")$2" || true
}

# val FILE FILTER: one value out of a JSON file as text, empty when absent.
val() { jq -r "($2) // empty | tostring" "$1" 2>/dev/null || true; }

# row LABEL A B [EXPECT]: ok when both statuses match (and, with EXPECT, both are EXPECT).
row() {
  local verdict=DIFF
  if [ "$2" = "$3" ] && [ "$2" != - ] && [ "$2" != 000 ] && { [ -z "${4:-}" ] || [ "$2" = "$4" ]; }; then
    verdict=ok
  else
    diffs=$((diffs + 1))
  fi
  printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$verdict"
}

skip() { printf '%s\t-\t-\tSKIP %s\n' "$1" "$2"; }

# pair2 LABEL METHOD PATH_A PATH_B BODY_A BODY_B [EXPECT]: the request on A, then on B. An empty path means
# that side has no id from its own earlier response: nothing is sent there and its status reads `-`.
# Afterwards $tmp/a.json and $tmp/b.json hold the two response bodies.
pair2() {
  n=$((n + 1))
  sa=-
  sb=-
  : >"$tmp/a.json"
  : >"$tmp/b.json"
  if [ -n "$3" ]; then
    send a "$2" "$3" "$5"
    sa=$code
  fi
  if [ -n "$4" ]; then
    send b "$2" "$4" "$6"
    sb=$code
  fi
  row "$1" "$sa" "$sb" "${7:-}"
}

# pair LABEL METHOD PATH_A PATH_B [BODY]: the same body on both sides.
pair() { pair2 "$1" "$2" "$3" "$4" "${5:-}" "${5:-}"; }

# at SIDE FILTER: a value from SIDE's last write response.
at() { val "$tmp/$1.json" "$2"; }

# path PREFIX ID [SUFFIX]: PREFIX/ID/SUFFIX, or empty when ID is.
path() { if [ -n "$2" ]; then echo "$1$2${3:-}"; fi; }

# coal_pdf FILE: the two-page PDF that finance-app tests/research_extract.rs builds with coal_pdf().
coal_pdf() {
  local f=$1 i c xref objs=() offs=()
  objs+=('<< /Type /Catalog /Pages 2 0 R >>')
  objs+=('<< /Type /Pages /Kids [4 0 R 6 0 R] /Count 2 >>')
  objs+=('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>')
  c=$'BT /F1 12 Tf 72 720 Td 18 TL\n(Kinerja operasional Q2 2026) Tj T*\n(Produksi 12,5 juta ton) Tj T*\n(Penjualan 11.200 kt) Tj T*\nET'
  objs+=('<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 5 0 R >>')
  objs+=("<< /Length ${#c} >>"$'\nstream\n'"$c"$'\nendstream')
  c=$'BT /F1 12 Tf 72 720 Td 18 TL\n(Average selling price USD 65,3/t) Tj T*\n(Cash cost 41,0 USD/t) Tj T*\n(CAR 23,4%) Tj T*\n(Coal reserves 120 Mt) Tj T*\nET'
  objs+=('<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents 7 0 R >>')
  objs+=("<< /Length ${#c} >>"$'\nstream\n'"$c"$'\nendstream')
  printf '%%PDF-1.4\n' >"$f"
  for i in 0 1 2 3 4 5 6; do
    offs+=("$(wc -c <"$f" | tr -d ' ')")
    printf '%d 0 obj\n%s\nendobj\n' $((i + 1)) "${objs[$i]}" >>"$f"
  done
  xref=$(wc -c <"$f" | tr -d ' ')
  {
    printf 'xref\n0 8\n%010d 65535 f \n' 0
    for i in "${offs[@]}"; do printf '%010d 00000 n \n' "$i"; done
    printf 'trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n' "$xref"
  } >>"$f"
}

T=$TICKER
R=/api/research
U=BBBB
[ "$T" != BBBB ] || U=CCCC

# Bodies are copied from the finance-app (4ef2dd9) test named on each step, its ticker replaced by
# $T (or $U), so the expected status is the one that test asserts.

# 1-3 research_universe_api.rs. Step 2 pins the coal pack, as research_kpi.rs pins COAL, so steps 4,
# 13 and 14 meet pack metrics on any ticker; BUMI's own pack already is coal_mining.
pair "01 POST $R/universe ($U)" POST "$R/universe" "$R/universe" \
  "{\"ticker\": \"$U\", \"pack_override\": \"it_distribution\", \"business_note\": \"Distributor sintetis\"}"
ua=$(path "$R/universe/" "$(at a .ticker)")
ub=$(path "$R/universe/" "$(at b .ticker)")
pair "02 PUT $R/universe/$T" PUT "$R/universe/$T" "$R/universe/$T" \
  '{"pack_override": "coal_mining", "business_note": "Baru"}'
pair "03 DELETE $R/universe/{step 1}" DELETE "$ua" "$ub"

# 4-5 research_kpi.rs: the DELETE names the cell the PUT answered with.
pair "04 PUT $R/$T/kpi/production_mt/2026/2" PUT "$R/$T/kpi/production_mt/2026/2" "$R/$T/kpi/production_mt/2026/2" \
  '{"value": 7.25, "document": "Laporan operasional Q2 2026, hal. 4", "document_date": "2026-07-30"}'
cell='select(.metric and .fy and .q) | "\(.metric)/\(.fy)/\(.q)"'
ka=$(path "$R/$T/kpi/" "$(at a "$cell")")
kb=$(path "$R/$T/kpi/" "$(at b "$cell")")
pair "05 DELETE $R/$T/kpi/{step 4}" DELETE "$ka" "$kb"

# 6-7 research_claims.rs
pair "06 POST $R/$T/claims" POST "$R/$T/claims" "$R/$T/claims" \
  '{"statement": "Ekuitas positif", "metric": "core.total_equity", "comparator": ">", "threshold": 0.0, "window": "latest", "due": "2027-03-31"}'
ca=$(at a .id)
cb=$(at b .id)
pair "07 PATCH $R/claims/{step 6}" PATCH "$(path "$R/claims/" "$ca")" "$(path "$R/claims/" "$cb")" \
  '{"closed": true}'

# 8-9 research_documents.rs
pair "08 POST $R/$T/documents" POST "$R/$T/documents" "$R/$T/documents" \
  '{"kind": "artikel", "url": "https://example.test/berita"}'
da=$(at a .id)
db=$(at b .id)
ea=
eb=
[ -z "$ca" ] || [ -z "$da" ] || ea=$R/evidence
[ -z "$cb" ] || [ -z "$db" ] || eb=$R/evidence
pair2 "09 POST $R/evidence (steps 6, 8)" POST "$ea" "$eb" \
  "{\"document_id\": \"$da\", \"claim_id\": \"$ca\"}" "{\"document_id\": \"$db\", \"claim_id\": \"$cb\"}"

# 10-11 research_journal.rs
pair "10 POST $R/$T/journal" POST "$R/$T/journal" "$R/$T/journal" \
  '{"kind": "keputusan", "text": "Menambah posisi"}'
pair "11 POST $R/journal/{step 10}/annotations" POST \
  "$(path "$R/journal/" "$(at a .id)" /annotations)" "$(path "$R/journal/" "$(at b .id)" /annotations)" \
  '{"text": "Ternyata laporan kuartal 3 lebih lemah"}'

# 12 research_events.rs: the first open event on each side's desk, read after the writes above.
get a "$R/desk"
xa=$(val "$tmp/a.get.json" '.inbox[0].id')
get b "$R/desk"
xb=$(val "$tmp/b.get.json" '.inbox[0].id')
label="12 POST $R/events/{first open}/dismiss"
if [ -z "$xa" ] && [ -z "$xb" ]; then
  skip "$label" "no open event"
else
  pair "$label" POST "$(path "$R/events/" "$xa" /dismiss)" "$(path "$R/events/" "$xb" /dismiss)" '{}'
fi

# 13 research_extract.rs: the upload always runs; accept and reject take the first two pending
# proposals each side lists after it.
coal_pdf "$tmp/paparan-q2.pdf"
pair "13 POST $R/$T/documents/upload" POST "$R/$T/documents/upload" "$R/$T/documents/upload" --form
get a "$R/$T/proposals?status=pending"
pa1=$(val "$tmp/a.get.json" '.items[0].id')
pa2=$(val "$tmp/a.get.json" '.items[1].id')
get b "$R/$T/proposals?status=pending"
pb1=$(val "$tmp/b.get.json" '.items[0].id')
pb2=$(val "$tmp/b.get.json" '.items[1].id')
[ -n "$pa2" ] || pa1=
[ -n "$pb2" ] || pb1=
[ -n "$pa1" ] || pa2=
[ -n "$pb1" ] || pb2=
if [ -z "$pa1" ] && [ -z "$pb1" ]; then
  skip "13 POST $R/proposals/{first}/accept" "fewer than 2 proposals"
  skip "13 POST $R/proposals/{second}/reject" "fewer than 2 proposals"
else
  pair "13 POST $R/proposals/{first}/accept" POST \
    "$(path "$R/proposals/" "$pa1" /accept)" "$(path "$R/proposals/" "$pb1" /accept)" '{}'
  pair "13 POST $R/proposals/{second}/reject" POST \
    "$(path "$R/proposals/" "$pa2" /reject)" "$(path "$R/proposals/" "$pb2" /reject)" '{}'
fi

# 14 research_structure.rs: each side imports its own rewrite of the BUMI fixture (plan.md §3.1b);
# the fixture itself stays byte for byte.
for DATA in "$A_DATA" "$B_DATA"; do
  mkdir -p "$DATA/research/structure"
  jq --arg t "$TICKER" '.ticker = $t | if $t == "BUMI" then . else .nodes |= map(del(.metric)) end' \
    tests/fixtures/research/structure-BUMI.draft.json > "$DATA/research/structure/$TICKER.draft.json"
done
pair "14 POST $R/$T/structure/draft" POST "$R/$T/structure/draft" "$R/$T/structure/draft" '{}'
draft_a=$sa
draft_b=$sb

# 15 research_structure.rs: accept the first node of the map, reject the second.
get a "$R/$T/structure"
na1=$(val "$tmp/a.get.json" '.nodes[0].id')
na2=$(val "$tmp/a.get.json" '.nodes[1].id')
get b "$R/$T/structure"
nb1=$(val "$tmp/b.get.json" '.nodes[0].id')
nb2=$(val "$tmp/b.get.json" '.nodes[1].id')
if [ -z "$na1$na2$nb1$nb2" ] && [ "${draft_a#2}" = "$draft_a" ] && [ "${draft_b#2}" = "$draft_b" ]; then
  skip "15 PUT $R/$T/structure/nodes/{first}" "import failed on both sides"
  skip "15 PUT $R/$T/structure/nodes/{second}" "import failed on both sides"
else
  pair "15 PUT $R/$T/structure/nodes/{first}" PUT \
    "$(path "$R/$T/structure/nodes/" "$na1")" "$(path "$R/$T/structure/nodes/" "$nb1")" '{"state": "accepted"}'
  pair "15 PUT $R/$T/structure/nodes/{second}" PUT \
    "$(path "$R/$T/structure/nodes/" "$na2")" "$(path "$R/$T/structure/nodes/" "$nb2")" '{"state": "rejected"}'
fi

# 16 research_structure.rs: an owner node, then its removal.
pair "16 POST $R/$T/structure/nodes" POST "$R/$T/structure/nodes" "$R/$T/structure/nodes" \
  '{"label": "Lain-lain"}'
pair "16 DELETE $R/$T/structure/nodes/{step 16}" DELETE \
  "$(path "$R/$T/structure/nodes/" "$(at a .node.id)")" "$(path "$R/$T/structure/nodes/" "$(at b .node.id)")"

# 17 research_valuation.rs
pair "17 PUT $R/$T/scenarios" PUT "$R/$T/scenarios" "$R/$T/scenarios" \
  '{"bear": {"growth": 0.0, "margin": 8.0, "exit_multiple": 8.0, "years": 5}, "base": {"growth": 8.0, "margin": 10.0, "exit_multiple": 12.0, "years": 5}, "bull": {"growth": 15.0, "margin": 12.0, "exit_multiple": 15.0, "years": 5}}'

# 18-19 research_api.rs. The DELETE names the PUT's as_of, so other manual counts stay.
pair "18 POST $R/$T/seen" POST "$R/$T/seen" "$R/$T/seen"
pair "19 PUT $R/$T/shares" PUT "$R/$T/shares" "$R/$T/shares" \
  '{"shares": 2600000.0, "as_of": "2026-09-30", "source_note": "rights issue"}'
pair "19 DELETE $R/$T/shares?as_of={step 19}" DELETE \
  "$(path "$R/$T/shares?as_of=" "$(at a .as_of)")" "$(path "$R/$T/shares?as_of=" "$(at b .as_of)")"

# 20 research_calendar.rs: the DELETE names the date and kind the POST answered with.
pair "20 POST $R/$T/calendar" POST "$R/$T/calendar" "$R/$T/calendar" \
  '{"date": "2026-11-15", "kind": "rups", "note": "  RUPS luar biasa  "}'
day='select(.date and .kind) | "\(.date)/\(.kind)"'
pair "20 DELETE $R/$T/calendar/{date}/{kind}" DELETE \
  "$(path "$R/$T/calendar/" "$(at a "$day")")" "$(path "$R/$T/calendar/" "$(at b "$day")")"

# 21 research_packs.rs
pair "21 POST $R/packs/reload" POST "$R/packs/reload" "$R/packs/reload"

# 22 research_api.rs: 503 on both while the scheduler is off (D19). Both sides returning 202 (a
# scheduler running, so a real refresh) is a DIFF, not a match.
pair2 "22 POST $R/refresh?ticker=$T" POST "$R/refresh?ticker=$T" "$R/refresh?ticker=$T" "" "" 503

# 23 thesis.rs
pair "23 POST /api/thesis/drafts" POST /api/thesis/drafts /api/thesis/drafts \
  '{"ticker": "test", "title": "ACCEPTANCE", "thesis_statement": "x", "falsifiers": ["y"]}'
pair "23 DELETE /api/thesis/drafts/{step 23}" DELETE \
  "$(path /api/thesis/drafts/ "$(at a .id)")" "$(path /api/thesis/drafts/ "$(at b .id)")"

# 24 macro_watch.rs
pair "24 POST /api/macro/trigger" POST /api/macro/trigger /api/macro/trigger

echo "writes: $diffs differences"
[ "$diffs" -eq 0 ]
