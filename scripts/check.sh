#!/usr/bin/env bash
# One gate for humans, agents and CI: scripts/check.sh [hygiene|backend|web|e2e|all]
# hygiene reads a commit, not the working tree: CHECK_REV (default: the index, i.e. what the next commit holds).
# The Co-Authored-By scan reads CHECK_RANGE (default: merge-base with ${CHECK_BASE:-origin/main}..HEAD).
# backend/web/e2e skip until their code exists, and fail when code exists without its root manifest.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

fail=0
bad() { echo "FAIL: $*" >&2; fail=1; }
warn() { echo "WARN: $*" >&2; }

# grep_rev <message> <git grep args>: a match is a failure, and so is a git error (rc > 1).
grep_rev() {
  local msg=$1 rc=0
  shift
  git grep "$@" || rc=$?
  case $rc in
    0) bad "$msg" ;;
    1) ;;
    *) bad "git grep error $rc ($msg)" ;;
  esac
}

# path_hits <message> <regex> [exclude regex]: tracked paths of the checked commit that match.
path_hits() {
  local out
  out=$(printf '%s\n' "$paths" | grep -E "$2" | grep -vE "${3:-^$}" || true)
  if [ -n "$out" ]; then echo "$out"; bad "$1"; fi
}

coauthor_scan() {
  local range=${CHECK_RANGE:-} base msgs args
  if [ -z "$range" ]; then
    base=$(git merge-base HEAD "${CHECK_BASE:-origin/main}" 2>/dev/null || true)
    if [ -z "$base" ]; then
      if [ "${CI:-}" = true ]; then
        bad "no base ref ${CHECK_BASE:-origin/main}: Co-Authored-By scan cannot run"
      else
        warn "no base ref ${CHECK_BASE:-origin/main} (git fetch origin): Co-Authored-By scan skipped"
      fi
      return
    fi
    range="$base..HEAD"
  fi
  read -r -a args <<<"$range"
  if ! msgs=$(git log --format=%B "${args[@]}"); then
    bad "git log $range failed: Co-Authored-By scan cannot run"
    return
  fi
  if printf '%s\n' "$msgs" | grep -qiE '^[[:space:]]*co-authored-by[[:space:]]*:'; then
    bad "Co-Authored-By trailer in a commit on this branch ($range)"
  fi
}

hygiene() {
  local rev=${CHECK_REV:-} listing out
  [ -n "$rev" ] || rev=$(git write-tree 2>/dev/null) || rev=HEAD
  echo "== hygiene ($rev)"

  # "<size>\t<path>" per tracked file; -z keeps non-ASCII names unquoted.
  listing=$(git ls-tree -r -l -z --full-tree "$rev" | tr '\0' '\n' |
    awk '{ t = index($0, "\t"); split(substr($0, 1, t - 1), m, " "); print m[4] "\t" substr($0, t + 1) }')
  paths=$(printf '%s\n' "$listing" | cut -f2-)

  out=$(printf '%s\n' "$paths" | LC_ALL=C grep -n '[^ -~]' || true)
  if [ -n "$out" ]; then echo "$out"; bad "non-ASCII path (rename it; tooling and greps mis-handle it)"; fi
  path_hits ".env file tracked (secrets stay in ~/.hermes/secrets)" '(^|/)(\.env(\..+)?|[^/]+\.env|\.envrc)$' '(^|/)\.env\.example$'
  path_hits "key or credential file tracked" '\.(p12|pfx|pem|key)$|service-account[^/]*\.json$|(^|/)id_(rsa|ecdsa|ed25519)$'
  path_hits "database file tracked (live data stays in the data dir)" '\.(sqlite3?|db)(-(wal|shm|journal)|\.bak)?$'
  out=$(printf '%s\n' "$listing" | awk -F'\t' '$1 ~ /^[0-9]+$/ && $1 > 1048576 && $2 !~ /^web\/public\/icons\// { print $1 "\t" $2 }')
  if [ -n "$out" ]; then echo "$out"; bad "file over 1 MB (only web/public/icons/ may be)"; fi

  grep_rev "private key material committed" -nE 'BEGIN ([A-Z]+ )?PRIVATE KEY' "$rev" --
  grep_rev "token committed (app bearer, GitHub, Google, Anthropic)" -nE \
    "INVESTING_APP_TOKEN=[\"']?[0-9A-Za-z_-]{16,}|ghp_[0-9A-Za-z]{30,}|github_pat_[0-9A-Za-z_]{30,}|AIza[0-9A-Za-z_-]{30,}|sk-ant-[0-9A-Za-z_-]{20,}" "$rev" --

  # prd.md §6 renames: no finance-app identifiers in code. Docs and scripts/ (parity tooling talks to finance-app) are exempt.
  # The [x] brackets keep this file out of its own grep and out of plan.md §4's L1 grep.
  grep_rev "finance-app identifier in code (use INVESTING_*, investing_session, X-Investing-*)" \
    -nE 'FINANCE[_]|finance[_]session|X-Financ[e]' "$rev" -- ':!*.md' ':!scripts/'

  # Decision D1: finance-app live values only in scripts/; investing-app defaults only where plan §3.1 puts them.
  grep_rev "finance-app live data dir or port outside scripts/" \
    -nE 'finance-app-dat[a]|(127\.0\.0\.1|localhost):878[7]' "$rev" -- ':!*.md' ':!scripts/'
  grep_rev "investing-app default data dir or port outside src/config.rs, deploy/, scripts/{parity,counts,writes}.sh, web/vite.config.ts" \
    -nE 'investing-app-dat[a]|(127\.0\.0\.1|localhost):878[9]' "$rev" -- ':!*.md' ':!src/config.rs' ':!deploy/' \
    ':!scripts/parity.sh' ':!scripts/counts.sh' ':!scripts/writes.sh' ':!web/vite.config.ts'
  grep_rev "home path hard-coded (use \$HOME or a parameter)" -nE '/Users/[A-Za-z]' "$rev" -- ':!*.md'

  coauthor_scan

  if [ "${CI:-}" != true ] && [ "$(git config --get core.hooksPath || true)" != .githooks ]; then
    warn "git hooks are off in this clone; run git with -c core.hooksPath=.githooks (CLAUDE.md Guardrails)"
  fi
}

backend() {
  if [ ! -f Cargo.toml ]; then
    if [ -n "$(git ls-files '*.rs' '*Cargo.toml')" ]; then
      bad "Rust code without a root Cargo.toml (the crate lives at the repo root)"
    else
      echo "== backend: skipped (no Rust code yet)"
    fi
    return
  fi
  echo "== backend"
  [ -f Cargo.lock ] || bad "Cargo.lock missing (commit it; the gates run --locked)"
  cargo fmt --check || bad "cargo fmt"
  cargo clippy --locked --all-targets -- -D warnings || bad "cargo clippy"
  cargo test --locked --no-fail-fast || bad "cargo test"
}

web() {
  if [ ! -f web/package.json ]; then
    if [ -n "$(git ls-files web)" ]; then
      bad "web/ without web/package.json"
    else
      echo "== web: skipped (no web/ yet)"
    fi
    return
  fi
  echo "== web"
  (cd web && npm ci && npm test && npm run build) || bad "web gates"
}

# Decision D15: vite preview + page.route mocks, WebKit iPhone 15 project only (specs carry their own axe checks).
e2e() {
  if [ ! -f web/playwright.config.ts ]; then
    echo "== e2e: skipped (no web/playwright.config.ts yet)"
    return
  fi
  echo "== e2e"
  (cd web && { [ -d node_modules ] || npm ci; } && npm run build &&
    npx playwright install --with-deps webkit && npm run e2e -- --project=webkit-iphone15) || bad "e2e"
}

case "${1:-all}" in
  hygiene) hygiene ;;
  backend) backend ;;
  web) web ;;
  e2e) e2e ;;
  all) hygiene; backend; web; e2e ;;
  *) echo "usage: $0 [hygiene|backend|web|e2e|all]" >&2; exit 2 ;;
esac

if [ "$fail" != 0 ]; then echo "check.sh: failed" >&2; exit 1; fi
echo "OK"
