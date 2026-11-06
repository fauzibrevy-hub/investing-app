#!/usr/bin/env bash
# Tests for the release layout (deploy/release-lib.sh) and for deploy/run.sh started from it, with
# fake artifacts in a temp dir: no cargo, no launchctl, no network, nothing outside the temp dir.
# Not part of CI or scripts/check.sh; run it by hand after changing deploy/:
#
#   bash deploy/install.test.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source-path=SCRIPTDIR source=release-lib.sh
. "$here/release-lib.sh"

work=$(mktemp -d "${TMPDIR:-/tmp}/install-test.XXXXXX")
trap 'rm -rf "$work"' EXIT
work=$(cd "$work" && pwd -P) # run.sh reports physical paths

failures=0
pass() { printf 'ok    %s\n' "$1"; }
fail() {
  printf 'FAIL  %s\n' "$1"
  failures=$((failures + 1))
}
check() { # <name> <command...>
  local name=$1
  shift
  if "$@"; then pass "$name"; else fail "$name"; fi
}
matches() { [[ $1 =~ $2 ]]; } # <string> <extended regex>
same() { # <got> <want>
  [ "$1" = "$2" ] && return 0
  printf '      got  [%s]\n      want [%s]\n' "$1" "$2"
  return 1
}

# Fake build output. The "binary" prints where it runs from, the UI path and the packs dir run.sh
# gave it ("-" when unset).
build="$work/build"
mkdir -p "$build/web/dist"
cat >"$build/investing-app" <<'EOF'
#!/bin/sh
printf '%s|%s|%s\n' "$0" "$INVESTING_WEB_DIST" "${INVESTING_RESEARCH_PACKS_DIR:--}"
EOF
chmod 755 "$build/investing-app"
echo '<!doctype html><title>fake</title>' >"$build/web/dist/index.html"

env_file="$work/investing.env"
printf 'INVESTING_APP_TOKEN=x.dummy.not.a.secret\nINVESTING_BIND=127.0.0.1:9\n' >"$env_file"
packs="$work/checkout-packs"

root="$work/opt"
id() { printf '2026010%sT000000Z-fake00%s' "$1" "$1"; }
stage() { release_stage "$root" "$(id "$1")" "$build/investing-app" "$build/web/dist" "$here/run.sh" "sha$1" 2>/dev/null; }
cur() { release_link "$root" current; }
prev() { release_link "$root" previous; }
# As launchd starts it: the plist sets the env file and the packs dir.
run_current() { INVESTING_ENV_FILE="$env_file" INVESTING_RESEARCH_PACKS_DIR="$packs" "$root/current/run.sh"; }
releases() { find "$root/releases" -mindepth 1 -maxdepth 1 | sed "s|^$root/releases/||" | sort | tr '\n' ' '; }

# --- first and second install ---------------------------------------------------------------
stage 1 && release_activate "$root" "$(id 1)"
check "first install: current -> release 1" same "$(cur)" "$(id 1)"
check "first install: no previous" same "$(prev)" ""
check "links are relative" same "$(readlink "$root/current")" "releases/$(id 1)"
check "RELEASE file names the release and sha" \
  grep -qx "sha=sha1" "$root/releases/$(id 1)/RELEASE"
check "release holds bin, web/dist and run.sh" release_check "$root/releases/$(id 1)" investing-app

stage 2 && release_activate "$root" "$(id 2)"
check "second install: current -> release 2" same "$(cur)" "$(id 2)"
check "second install: previous -> release 1" same "$(prev)" "$(id 1)"
check "swap renames over the link (nothing moved into a release)" \
  same "$(find "$root/releases" -type l | wc -l | tr -d ' ')" "0"

# --- failed installs leave current alone --------------------------------------------------------
mv "$build/web/dist/index.html" "$work/index.html.away"
if stage 3; then fail "stage without index.html is refused"; else pass "stage without index.html is refused"; fi
mv "$work/index.html.away" "$build/web/dist/index.html"
check "  current unchanged, no release 3, no .tmp left" \
  same "$(cur) $(releases)" "$(id 2) $(id 1) $(id 2) "

chmod 644 "$build/investing-app"
if stage 3; then fail "stage with a non-executable binary is refused"; else pass "stage with a non-executable binary is refused"; fi
chmod 755 "$build/investing-app"
check "  current unchanged" same "$(cur) $(releases)" "$(id 2) $(id 1) $(id 2) "

if stage 2; then fail "staging an existing release id is refused"; else pass "staging an existing release id is refused"; fi
if release_activate "$root" "$(id 7)" 2>/dev/null; then fail "activating a missing release is refused"; else pass "activating a missing release is refused"; fi
check "  current unchanged" same "$(cur)" "$(id 2)"

# --- run.sh starts the current release, never the build output -----------------------------------
check "run.sh runs the current release's binary and UI, packs from the plist" same "$(run_current)" \
  "$root/releases/$(id 2)/bin/investing-app|$root/releases/$(id 2)/web/dist|$packs"
mv "$build" "$work/build.away"
check "build output gone (cargo clean, deleted web/dist): still runs" same "$(run_current)" \
  "$root/releases/$(id 2)/bin/investing-app|$root/releases/$(id 2)/web/dist|$packs"
mv "$work/build.away" "$build"
printf 'INVESTING_WEB_DIST=/elsewhere/dist\nINVESTING_RESEARCH_PACKS_DIR=/elsewhere/packs\n' >>"$env_file"
check "env-file INVESTING_WEB_DIST and INVESTING_RESEARCH_PACKS_DIR still win" \
  same "$(run_current | cut -d'|' -f2-)" "/elsewhere/dist|/elsewhere/packs"
/usr/bin/sed -i '' '/^INVESTING_WEB_DIST=/d; /^INVESTING_RESEARCH_PACKS_DIR=/d' "$env_file"
check "no packs dir from plist or env file: warns, runs with embedded packs" \
  same "$(INVESTING_ENV_FILE="$env_file" "$root/current/run.sh" 2>/dev/null | cut -d'|' -f3)" "-"
if INVESTING_ENV_FILE="$work/missing.env" "$root/current/run.sh" 2>/dev/null; then rc=0; else rc=$?; fi
check "run.sh exits 78 without an env file" same "$rc" "78"

# Checkout layout: the plist from before this change, and scripts/tools-selftest.sh step 11, run
# <checkout>/deploy/run.sh. That must still start the checkout's build with the checkout's packs.
mkdir -p "$work/checkout/deploy" "$work/checkout/target/release" "$work/checkout/web/dist"
cp "$here/run.sh" "$work/checkout/deploy/run.sh"
cp "$build/investing-app" "$work/checkout/target/release/investing-app"
check "run.sh from a checkout's deploy/ starts that checkout's build and packs" \
  same "$(INVESTING_ENV_FILE="$env_file" "$work/checkout/deploy/run.sh")" \
  "$work/checkout/target/release/investing-app|$work/checkout/web/dist|$work/checkout/research/packs"

# --- rollback ---------------------------------------------------------------------------------
release_rollback "$root" investing-app
check "rollback: current -> release 1, previous -> release 2" same "$(cur) $(prev)" "$(id 1) $(id 2)"
check "rollback: run.sh starts release 1" same "$(run_current | cut -d'|' -f1)" \
  "$root/releases/$(id 1)/bin/investing-app"
release_rollback "$root" investing-app
check "second rollback goes forward again" same "$(cur) $(prev)" "$(id 2) $(id 1)"

mv "$root/releases/$(id 1)/web/dist/index.html" "$work/index.html.away"
if release_rollback "$root" investing-app 2>/dev/null; then fail "rollback to an incomplete release is refused"; else pass "rollback to an incomplete release is refused"; fi
mv "$work/index.html.away" "$root/releases/$(id 1)/web/dist/index.html"
check "  current unchanged" same "$(cur) $(prev)" "$(id 2) $(id 1)"

solo="$work/solo"
release_stage "$solo" "$(id 1)" "$build/investing-app" "$build/web/dist" "$here/run.sh" sha1 &&
  release_activate "$solo" "$(id 1)"
if release_rollback "$solo" investing-app 2>/dev/null; then fail "rollback without previous is refused"; else pass "rollback without previous is refused"; fi
check "  current unchanged" same "$(release_link "$solo" current)" "$(id 1)"

# --- prune ------------------------------------------------------------------------------------
for n in 3 4 5; do stage "$n" && release_activate "$root" "$(id "$n")"; done
mkdir -p "$root/releases/$(id 9).tmp"
release_prune "$root" 3
check "prune keeps the newest 3 (current, previous, one more) and drops .tmp leftovers" \
  same "$(releases)" "$(id 3) $(id 4) $(id 5) "
check "  current and previous kept" same "$(cur) $(prev)" "$(id 5) $(id 4)"

# After a rollback current is older than previous; a new install then keeps the rolled-back-to
# release as previous, and prune must keep it even though it is not among the newest.
release_rollback "$root" investing-app # current 4, previous 5
stage 6 && release_activate "$root" "$(id 6)" # current 6, previous 4
release_prune "$root" 3
check "prune after a rollback keeps current, previous and the newest other" \
  same "$(releases)" "$(id 4) $(id 5) $(id 6) "
release_prune "$root" 3
check "prune is idempotent" same "$(releases)" "$(id 4) $(id 5) $(id 6) "

# --- release ids --------------------------------------------------------------------------------
check "release_id is <UTC ts>-<short sha>[-dirty]" \
  matches "$(release_id "$here/..")" '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7}(-dirty)?$'
mkdir -p "$work/nogit"
check "release_id outside git says nogit" \
  matches "$(release_id "$work/nogit")" '^[0-9]{8}T[0-9]{6}Z-nogit$'

echo
if [ "$failures" -ne 0 ]; then
  echo "$failures check(s) failed"
  exit 1
fi
echo "all checks passed"
