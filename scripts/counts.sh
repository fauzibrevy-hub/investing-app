#!/usr/bin/env bash
# scripts/counts.sh DIR_A DIR_B
# Compare per-ticker SQLite record counts and whole-directory metrics between two
# research data directories (plan.md §3.1b, prd.md S2 / A2).
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
    "$DIR/macro_store.json" \
    "$drafts"
  do
    if [ ! -f "$p" ]; then
      echo "counts.sh: missing file: $p" >&2
      exit 2
    fi
  done
  for p in \
    "$DIR/research/documents" \
    "$DIR/research/structure" \
    "$DIR/research/pdf-filings"
  do
    if [ ! -d "$p" ]; then
      echo "counts.sh: missing directory: $p" >&2
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

collect_dir() {
  local DIR="$1" drafts="$2"
  local log_lines idempotency_lines thesis_drafts macro_indicators macro_observations
  local files_documents files_structure files_pdf_filings

  log_lines="$(wc -l < "$DIR/research-log.jsonl" | tr -d '[:space:]')"
  idempotency_lines="$(wc -l < "$DIR/research-idempotency.jsonl" | tr -d '[:space:]')"
  thesis_drafts="$(jq 'length' "$drafts")"
  macro_indicators="$(jq 'length' "$DIR/macro_store.json")"
  macro_observations="$(jq '[.[] | (.history // []) | length] | add // 0' "$DIR/macro_store.json")"
  files_documents="$(find "$DIR/research/documents" -type f | wc -l | tr -d '[:space:]')"
  files_structure="$(find "$DIR/research/structure" -type f | wc -l | tr -d '[:space:]')"
  files_pdf_filings="$(find "$DIR/research/pdf-filings" -type f | wc -l | tr -d '[:space:]')"

  printf 'log_lines\t-\t%s\n' "$log_lines"
  printf 'idempotency_lines\t-\t%s\n' "$idempotency_lines"
  printf 'thesis_drafts\t-\t%s\n' "$thesis_drafts"
  printf 'macro_indicators\t-\t%s\n' "$macro_indicators"
  printf 'macro_observations\t-\t%s\n' "$macro_observations"
  printf 'files_documents\t-\t%s\n' "$files_documents"
  printf 'files_structure\t-\t%s\n' "$files_structure"
  printf 'files_pdf_filings\t-\t%s\n' "$files_pdf_filings"
}

if ! collect_sql "$DIR_A" > "$tmp_dir/a.sql" || ! collect_dir "$DIR_A" "$DRAFTS_A" > "$tmp_dir/a.dir"; then
  exit 2
fi
if ! collect_sql "$DIR_B" > "$tmp_dir/b.sql" || ! collect_dir "$DIR_B" "$DRAFTS_B" > "$tmp_dir/b.dir"; then
  exit 2
fi

{
  cut -f1,2 "$tmp_dir/a.sql" "$tmp_dir/b.sql" | LC_ALL=C sort -u
  cut -f1,2 "$tmp_dir/a.dir"
} > "$tmp_dir/keys"
cat "$tmp_dir/a.sql" "$tmp_dir/a.dir" > "$tmp_dir/a.tsv"
cat "$tmp_dir/b.sql" "$tmp_dir/b.dir" > "$tmp_dir/b.tsv"

rc=0
awk -F'\t' '
  FILENAME == ARGV[1] { a[$1 "\t" $2] = $3; next }
  FILENAME == ARGV[2] { b[$1 "\t" $2] = $3; next }
  {
    k = $1 "\t" $2
    av = (k in a && a[k] != "") ? a[k] : 0
    bv = (k in b && b[k] != "") ? b[k] : 0
    if (av != bv) {
      printf "%s\t%s\t%s\n", k, av, bv
      diffs++
    }
  }
  END {
    printf "counts: %d differences\n", diffs + 0
    exit (diffs > 0 ? 1 : 0)
  }
' "$tmp_dir/a.tsv" "$tmp_dir/b.tsv" "$tmp_dir/keys" || rc=$?

exit "$rc"
