#!/usr/bin/env bash
# Tests for the release layout (deploy/release-lib.sh), for deploy/run.sh started from it, and for
# the order of deploy/install.sh (what a refused install leaves behind), with fake artifacts in a
# temp dir: no cargo, no real launchctl (a fake one records calls), no network, nothing outside the
# temp dir (install.sh runs with HOME pointing into it).
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

# --- install.sh: refused installs change nothing -----------------------------------------------
# The real install.sh in a fake checkout (SKIP_BUILD=1), with HOME inside the temp dir and fake
# launchctl and lsof first on PATH. launchctl records its calls, says "not loaded" to `print` and
# fails everything else, so no job is ever loaded. lsof reports a foreign listener (pid 4242) on
# every call (LSOF_MODE=always), from the second call on (second: the port is taken while the
# build runs), or never.
repo="$work/repo"
mkdir -p "$repo/deploy" "$repo/target/release" "$repo/web/dist" "$repo/research/packs" "$work/bin"
cp "$here/install.sh" "$here/release-lib.sh" "$here/run.sh" "$here/com.menganto.investing.plist" "$repo/deploy/"
cp "$build/investing-app" "$repo/target/release/investing-app"
cp "$build/web/dist/index.html" "$repo/web/dist/"
touch "$repo/Cargo.toml"
echo '{}' >"$repo/web/package.json"
cat >"$work/bin/launchctl" <<'EOF'
#!/bin/sh
echo "$*" >>"$LAUNCHCTL_LOG"
[ "$1" = print ] && exit 113
exit 1
EOF
cat >"$work/bin/lsof" <<'EOF'
#!/bin/sh
n=$(($(cat "$LSOF_COUNT" 2>/dev/null || echo 0) + 1))
echo "$n" >"$LSOF_COUNT"
case "$LSOF_MODE" in
  always) echo 4242 ;;
  second) if [ "$n" -ge 2 ]; then echo 4242; fi ;;
esac
exit 0
EOF
cat >"$work/bin/cargo" <<'EOF'
#!/bin/sh
echo "unexpected: $0 $*" >&2
exit 1
EOF
cp "$work/bin/cargo" "$work/bin/npm"
chmod 755 "$work/bin/"*

ienv="$work/inst-env/investing.env"
reset_install() { # [env file INVESTING_BIND port, or nothing for no env file]
  rm -rf "${work:?}/inst" "$work/inst-data" "$work/inst-env" "${work:?}/home" "$work/launchctl.log" "$work/lsof.count"
  mkdir -p "$work/home"
  if [ -n "${1:-}" ]; then
    mkdir -p "$work/inst-env"
    printf 'INVESTING_APP_TOKEN=x\nPORTFOLIO_SPREADSHEET_ID=x\nINVESTING_BIND=127.0.0.1:%s\nINVESTING_DATA_DIR=%s\n' \
      "$1" "$work/inst-data" >"$ienv"
    cp "$ienv" "$work/env.before"
  fi
}
run_install() { # <LSOF_MODE> [NAME=value ...]: install.sh output; its exit code in $work/rc
  local mode=$1 rc=0
  shift
  env -i PATH="$work/bin:/usr/bin:/bin:/usr/sbin:/sbin" HOME="$work/home" TMPDIR="$work" \
    LAUNCHCTL_LOG="$work/launchctl.log" LSOF_COUNT="$work/lsof.count" LSOF_MODE="$mode" \
    INVESTING_LABEL=com.menganto.investing.test INVESTING_INSTALL_ROOT="$work/inst" \
    INVESTING_ENV_FILE="$ienv" INVESTING_DATA_DIR="$work/inst-data" SKIP_BUILD=1 \
    "$@" bash "$repo/deploy/install.sh" >"$work/out" 2>&1 || rc=$?
  echo "$rc" >"$work/rc"
}
refused_with() { [ "$(cat "$work/rc")" != 0 ] && grep -qF "$1" "$work/out"; }
no_release() { [ -z "$(release_link "$work/inst" current)" ] && [ -z "$(ls -A "$work/inst/releases" 2>/dev/null)" ]; }
no_plist() { [ -z "$(ls -A "$work/home/Library/LaunchAgents" 2>/dev/null)" ]; }
no_temp_plist() { ! ls "$work"/*.plist.* >/dev/null 2>&1; }
only_print_calls() { ! grep -qvE '^print ' "$work/launchctl.log" 2>/dev/null; }
env_unchanged() { cmp -s "$ienv" "$work/env.before"; }
label_refused() { [ "$(cat "$work/rc")" != 0 ] && grep -q "INVESTING_LABEL" "$work/out" && [ ! -e "$work/inst" ] && [ ! -e "$work/launchctl.log" ]; }
has_current_bin() { [ -x "$work/inst/current/bin/investing-app" ]; }

reset_install 18799
run_install always
check "port held: refused" refused_with "port 18799 is held by pid 4242"
check "  no release, no current" no_release
check "  no plist in LaunchAgents" no_plist
check "  launchctl only asked (print), never bootout/bootstrap" only_print_calls
check "  env file unchanged" env_unchanged

reset_install 18799
run_install second
check "port taken during the build: refused at the switch" refused_with "port 18799 is held by pid 4242"
check "  staged release removed, no current" no_release
check "  no plist in LaunchAgents" no_plist
check "  no leftover temp plist" no_temp_plist
check "  launchctl only asked (print)" only_print_calls

reset_install
run_install never
check "no env file: refused before anything is written" refused_with "needs its own INVESTING_DATA_DIR"
check "  no release, no plist" no_release
check "  no plist in LaunchAgents" no_plist

for bad in com.menganto.finance com.menganto.finance.staging com.menganto.investing.scraper \
  "com.menganto.investing.a b" com.menganto.investing. com.menganto.investingx; do
  reset_install 18799
  run_install never INVESTING_LABEL="$bad"
  check "INVESTING_LABEL=$bad refused, nothing written" label_refused
done

# Port free: the install gets as far as launchctl (fake, fails), so the switch and the plist are
# exercised: current is the new release, the plist is in place, bootstrap was called with it.
reset_install 18799
run_install never
check "port free: runs to launchctl bootstrap (fake, fails)" refused_with "launchctl bootstrap failed"
check "  current is the new release" has_current_bin
check "  plist in LaunchAgents with the test label" \
  grep -q '<string>com.menganto.investing.test</string>' "$work/home/Library/LaunchAgents/com.menganto.investing.test.plist"
check "  plist points the packs at the checkout" \
  grep -q "<string>$repo/research/packs</string>" "$work/home/Library/LaunchAgents/com.menganto.investing.test.plist"
check "  bootstrap was called with that plist" \
  grep -qF "bootstrap gui/$UID $work/home/Library/LaunchAgents/com.menganto.investing.test.plist" "$work/launchctl.log"

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
