#!/usr/bin/env bash
# scripts/counts.sh DIR_A DIR_B
# Compare per-ticker SQLite record counts and whole-directory metrics between two
# research data directories (plan.md §3.1b, prd.md S2 / A2).
#
# Read-only, writes nothing. When a directory has no research.sqlite-wal and no -shm, the
# database is opened with &immutable=1 (SQLite then neither creates those files nor fails on a
# closed WAL / .backup copy). Immutable mode assumes NO WRITER: run against stopped snapshot
# copies or a stopped server's dir. On a dir a running server writes to, a checkpointed idle
# database can read a stale view; stop the server first (plan.md §3.1b deviation, review L1b #7).
#
# Two things finance-app only creates on first use may be absent, on either side: the drafts file
# (its loader treats NotFound as an empty map, src/thesis/drafts.rs:31) and the research/{documents,
# structure,pdf-filings} directories. Absent counts as 0 and is printed as a `note` line ahead of
# the differences, so the evidence file shows it. A drafts file that exists but is not valid
# JSON, or a path that exists but is not the right kind of file, still exits 2.
set -euo pipefail

usage() {
  echo "usage: $0 DIR_A DIR_B" >&2
  exit 2
}

[ "$#" -eq 2 ] || usage
DIR_A="$1"
DIR_B="$2"
if [ -z "$DIR_A" ] || [ -z "$DIR_B" ]; then
  usage
fi

DRAFTS_A="${DRAFTS_A:-$DIR_A/thesis_drafts.json}"
DRAFTS_B="${DRAFTS_B:-$DIR_B/thesis_drafts.json}"

check_side() {
  local DIR="$1" drafts="$2" p
  if [ ! -d "$DIR" ]; then
    echo "counts.sh: missing directory: $DIR" >&2
    exit 2
  fi
  for p in \
    "$DIR/research.sqlite" \
    "$DIR/research-log.jsonl" \
    "$DIR/research-idempotency.jsonl" \
    "$DIR/macro_store.json"
  do
    if [ ! -f "$p" ]; then
      echo "counts.sh: missing file: $p" >&2
      exit 2
    fi
  done
  # Absent is fine (collect_dir counts it as 0); present but the wrong kind is not.
  if [ -e "$drafts" ] && [ ! -f "$drafts" ]; then
    echo "counts.sh: not a file: $drafts" >&2
    exit 2
  fi
  for p in \
    "$DIR/research/documents" \
    "$DIR/research/structure" \
    "$DIR/research/pdf-filings"
  do
    if [ -e "$p" ] && [ ! -d "$p" ]; then
      echo "counts.sh: not a directory: $p" >&2
      exit 2
    fi
  done
}

check_side "$DIR_A" "$DRAFTS_A"
check_side "$DIR_B" "$DRAFTS_B"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

collect_sql() {
  local DIR="$1"
  local uri="file:$DIR/research.sqlite?mode=ro"
  if [ ! -e "$DIR/research.sqlite-wal" ] && [ ! -e "$DIR/research.sqlite-shm" ]; then
    uri="file:$DIR/research.sqlite?mode=ro&immutable=1"
  fi
  sqlite3 -readonly -tabs "$uri" <<'SQL'
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
SQL
}

# count_files SIDE DIR NAME: files under DIR, or 0 with a note when DIR was never created.
count_files() {
  if [ -d "$2" ]; then
    find "$2" -type f | wc -l | tr -d '[:space:]'
  else
    echo 0
    printf 'note\t%s\t%s\tmissing (0)\n' "$1" "$3" >>"$tmp_dir/notes"
  fi
}

collect_dir() {
  local SIDE="$1" DIR="$2" drafts="$3"
  local log_lines idempotency_lines thesis_drafts macro_indicators macro_observations
  local files_documents files_structure files_pdf_filings

  log_lines="$(wc -l < "$DIR/research-log.jsonl" | tr -d '[:space:]')" || return 1
  idempotency_lines="$(wc -l < "$DIR/research-idempotency.jsonl" | tr -d '[:space:]')" || return 1
  if [ -e "$drafts" ]; then
    thesis_drafts="$(jq 'length' "$drafts")" || return 1
  else
    thesis_drafts=0
    printf 'note\t%s\tthesis_drafts\tmissing (0)\n' "$SIDE" >>"$tmp_dir/notes"
  fi
  macro_indicators="$(jq 'length' "$DIR/macro_store.json")" || return 1
  macro_observations="$(jq '[.[] | (.history // []) | length] | add // 0' "$DIR/macro_store.json")" || return 1
  files_documents="$(count_files "$SIDE" "$DIR/research/documents" files_documents)" || return 1
  files_structure="$(count_files "$SIDE" "$DIR/research/structure" files_structure)" || return 1
  files_pdf_filings="$(count_files "$SIDE" "$DIR/research/pdf-filings" files_pdf_filings)" || return 1

  printf 'log_lines\t-\t%s\n' "$log_lines"
  printf 'idempotency_lines\t-\t%s\n' "$idempotency_lines"
  printf 'thesis_drafts\t-\t%s\n' "$thesis_drafts"
  printf 'macro_indicators\t-\t%s\n' "$macro_indicators"
  printf 'macro_observations\t-\t%s\n' "$macro_observations"
  printf 'files_documents\t-\t%s\n' "$files_documents"
  printf 'files_structure\t-\t%s\n' "$files_structure"
  printf 'files_pdf_filings\t-\t%s\n' "$files_pdf_filings"
}

if ! collect_sql "$DIR_A" > "$tmp_dir/a.sql" || ! collect_dir A "$DIR_A" "$DRAFTS_A" > "$tmp_dir/a.dir"; then
  exit 2
fi
if ! collect_sql "$DIR_B" > "$tmp_dir/b.sql" || ! collect_dir B "$DIR_B" "$DRAFTS_B" > "$tmp_dir/b.dir"; then
  exit 2
fi

{
  cut -f1,2 "$tmp_dir/a.sql" "$tmp_dir/b.sql" | LC_ALL=C sort -u
  cut -f1,2 "$tmp_dir/a.dir"
} > "$tmp_dir/keys"
cat "$tmp_dir/a.sql" "$tmp_dir/a.dir" > "$tmp_dir/a.tsv"
cat "$tmp_dir/b.sql" "$tmp_dir/b.dir" > "$tmp_dir/b.tsv"

# One line per absent thing: note<TAB>side<TAB>metric<TAB>missing (0). Counted as 0, never a failure.
[ ! -s "$tmp_dir/notes" ] || LC_ALL=C sort "$tmp_dir/notes"

rc=0
awk -F'\t' '
  FILENAME == ARGV[1] { a[$1 "\t" $2] = $3; next }
  FILENAME == ARGV[2] { b[$1 "\t" $2] = $3; next }
  {
    k = $1 "\t" $2
    # an empty value is a failed query, never a 0
    if ((k in a && a[k] == "") || (k in b && b[k] == "")) bad = 1
    av = (k in a) ? a[k] : 0
    bv = (k in b) ? b[k] : 0
    if (av != bv) {
      printf "%s\t%s\t%s\n", k, av, bv
      diffs++
    }
  }
  END {
    if (bad) {
      print "counts.sh: an empty count (failed query)" > "/dev/stderr"
      exit 2
    }
    printf "counts: %d differences\n", diffs + 0
    exit (diffs > 0 ? 1 : 0)
  }
' "$tmp_dir/a.tsv" "$tmp_dir/b.tsv" "$tmp_dir/keys" || rc=$?

exit "$rc"
