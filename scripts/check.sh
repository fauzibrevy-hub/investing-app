#!/usr/bin/env bash
# One gate for humans, agents and CI: scripts/check.sh [hygiene|backend|web|all]
# Each part skips itself until its code exists (Cargo.toml lands in L1, web/ in L2).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail=0
bad() { echo "FAIL: $*" >&2; fail=1; }

hygiene() {
  echo "== hygiene"
  git grep -nE 'BEGIN ([A-Z]+ )?PRIVATE KEY' && bad "private key material committed"

  git ls-files | grep -E '(^|/)\.env(\..*)?$' | grep -v '\.env\.example$' && bad ".env file tracked"
  git ls-files | grep -E '\.(sqlite|sqlite-.*|db)$' && bad "database file tracked (live data stays in ~/investing-app-data)"

  # prd.md §6 renames: no finance-app identifiers in code. Docs and scripts/ (parity tooling talks to finance-app) are exempt.
  git grep -nE 'FINANCE_|finance_session|X-Finance' -- ':!*.md' ':!scripts/' && bad "finance-app identifier in code (use INVESTING_*, investing_session, X-Investing-*)"

  # plan.md §1 live data rule + §3.1 deploy templates: no live data dirs, live ports or home paths in code.
  git grep -nE 'finance-app-data|investing-app-data|127\.0\.0\.1:87(87|89)|/Users/[a-z]' -- ':!*.md' ':!scripts/check.sh' \
    && bad "live data path, live port or home path hard-coded"

  # CLAUDE.md: no Co-Authored-By trailers on branch commits.
  local base
  base=$(git merge-base HEAD "${CHECK_BASE:-origin/main}" 2>/dev/null || true)
  if [ -n "$base" ] && git log --format=%B "$base..HEAD" | grep -qi '^co-authored-by:'; then
    bad "Co-Authored-By trailer in a commit on this branch"
  fi
}

backend() {
  [ -f Cargo.toml ] || { echo "== backend: skipped (no Cargo.toml yet)"; return; }
  echo "== backend"
  cargo fmt --check || bad "cargo fmt"
  cargo clippy --locked --all-targets -- -D warnings || bad "cargo clippy"
  cargo test --locked --no-fail-fast || bad "cargo test"
}

web() {
  [ -f web/package.json ] || { echo "== web: skipped (no web/package.json yet)"; return; }
  echo "== web"
  (cd web && npm ci && npm test && npm run build) || bad "web gates"
}

case "${1:-all}" in
  hygiene) hygiene ;;
  backend) backend ;;
  web) web ;;
  all) hygiene; backend; web ;;
  *) echo "usage: $0 [hygiene|backend|web|all]" >&2; exit 2 ;;
esac

if [ "$fail" != 0 ]; then echo "check.sh: failed" >&2; exit 1; fi
echo "OK"
