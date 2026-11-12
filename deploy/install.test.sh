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
mkdir -p "$work/runhome/.secrets"
cp "$env_file" "$work/runhome/.secrets/investing-app.env"
if out=$(env -u INVESTING_ENV_FILE HOME="$work/runhome" "$root/current/run.sh" 2>/dev/null); then rc=0; else rc=$?; fi
check "run.sh without INVESTING_ENV_FILE reads \$HOME/.secrets/investing-app.env" same "$rc:${out%%|*}" \
  "0:$root/releases/$(id 2)/bin/investing-app"

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

# `previous` cannot be set after `current` moved: return 2 (not 1), so a caller knows `current` is
# the new release and must not delete it.
ro="$work/ro"
release_stage "$ro" "$(id 1)" "$build/investing-app" "$build/web/dist" "$here/run.sh" sha1 && release_activate "$ro" "$(id 1)"
release_stage "$ro" "$(id 2)" "$build/investing-app" "$build/web/dist" "$here/run.sh" sha2
mkdir "$ro/previous"
chmod 555 "$ro/previous"
if release_activate "$ro" "$(id 2)" 2>/dev/null; then rc=0; else rc=$?; fi
chmod 755 "$ro/previous"
check "release_activate: previous not settable returns 2, current already moved" \
  same "$rc $(release_link "$ro" current)" "2 $(id 2)"

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
# print: "loaded" for LAUNCHCTL_LINGER more calls after a bootout (launchd removes a job
# asynchronously), then "not loaded" (113). bootstrap always fails, with launchd's error text.
echo "$*" >>"$LAUNCHCTL_LOG"
case "$1" in
  print)
    n=$(cat "$LAUNCHCTL_STATE" 2>/dev/null || echo 0)
    if [ "$n" -gt 0 ]; then
      echo $((n - 1)) >"$LAUNCHCTL_STATE"
      exit 0
    fi
    exit 113 ;;
  bootout) echo "${LAUNCHCTL_LINGER:-0}" >"$LAUNCHCTL_STATE" ;;
  bootstrap) echo "Bootstrap failed: 5: Input/output error" >&2; exit 5 ;;
  *) exit 1 ;;
esac
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
# The account's passwd entry (id -P) names FAKE_LIVE_HOME as its home, so the paths the staging
# guard treats as live are temp paths too; every other id call is the real one. FAKE_ID_MODE=fail
# makes id -P fail, =empty leaves the home field empty, =garbage prints a line that is no entry.
cat >"$work/bin/id" <<'EOF'
#!/bin/sh
if [ "$1" = -P ]; then
  case "${FAKE_ID_MODE:-}" in
    fail) exit 1 ;;
    empty) echo "tester:********:501:20::0:0:Tester::/bin/sh" ;;
    garbage) echo "garbage" ;;
    *) echo "tester:********:501:20::0:0:Tester:$FAKE_LIVE_HOME:/bin/sh" ;;
  esac
  exit 0
fi
exec /usr/bin/id "$@"
EOF
chmod 755 "$work/bin/"*

ienv="$work/inst-env/investing.env"
reset_install() { # [env file INVESTING_BIND port, or nothing for no env file]
  rm -rf "${work:?}/inst" "$work/inst-data" "$work/inst-env" "${work:?}/home" "${work:?}/livehome" \
    "$work/links" "$work/launchctl.log" "$work/lsof.count" "$work/launchctl.state"
  mkdir -p "$work/home"
  if [ -n "${1:-}" ]; then
    mkdir -p "$work/inst-env"
    printf 'INVESTING_APP_TOKEN=x\nPORTFOLIO_SPREADSHEET_ID=x\nINVESTING_BIND=127.0.0.1:%s\nINVESTING_DATA_DIR=%s\n' \
      "$1" "$work/inst-data" >"$ienv"
    cp "$ienv" "$work/env.before"
  fi
}
run_install() { # <LSOF_MODE> [NAME=value ...]: install.sh output; its exit code in $work/rc
  # INVESTING_ENV_FILE is $ienv, or unset (the default under HOME) with UNSET_ENV_FILE=1.
  # INSTALL_ARG=--rollback runs a rollback.
  local mode=$1 rc=0 envfile="INVESTING_ENV_FILE=$ienv"
  shift
  [ "${UNSET_ENV_FILE:-0}" = 1 ] && envfile="UNUSED_BY_INSTALL=1"
  env -i PATH="$work/bin:/usr/bin:/bin:/usr/sbin:/sbin" HOME="$work/home" TMPDIR="$work" \
    LAUNCHCTL_LOG="$work/launchctl.log" LAUNCHCTL_STATE="$work/launchctl.state" LSOF_COUNT="$work/lsof.count" LSOF_MODE="$mode" \
    FAKE_LIVE_HOME="$work/livehome" INVESTING_LABEL=com.menganto.investing.test INVESTING_INSTALL_ROOT="$work/inst" \
    "$envfile" INVESTING_DATA_DIR="$work/inst-data" SKIP_BUILD=1 \
    "$@" bash "$repo/deploy/install.sh" ${INSTALL_ARG:+"$INSTALL_ARG"} >"$work/out" 2>&1 || rc=$?
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
seed_current() { # <release id>: an existing install root whose current is that release
  release_stage "$work/inst" "$1" "$build/investing-app" "$build/web/dist" "$here/run.sh" seed 2>/dev/null &&
    release_activate "$work/inst" "$1"
}
bootstrap_tried_3() {
  [ "$(grep -c '^bootstrap ' "$work/launchctl.log")" = 3 ] &&
    grep -q 'try 3 of 3): Bootstrap failed: 5: Input/output error' "$work/out"
}
hint_is_runbook() { grep -qF 'RUNBOOK §1 "First install with the install root"' "$work/out" && ! grep -q -- '--rollback' "$work/out"; }
hint_is_rollback() { grep -q 'launchctl bootstrap failed; roll back: ./deploy/install.sh --rollback' "$work/out"; }
warned_previous() { grep -q 'WARN: previous was not updated' "$work/out" && grep -q 'launchctl bootstrap failed' "$work/out"; }
current_not_old1() { [ "$(readlink "$work/inst/current")" != releases/old1 ]; }
prints_between_bootout_and_bootstrap() { # <n>
  [ "$(awk '/^bootout /{on=1; n=0; next} on && /^print /{n++} on && /^bootstrap /{print n; exit}' "$work/launchctl.log")" = "$1" ]
}

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
run_install never LAUNCHCTL_LINGER=3
check "port free: runs to launchctl bootstrap (fake, fails)" refused_with "launchctl bootstrap failed"
check "  current is the new release" has_current_bin
check "  plist in LaunchAgents with the test label" \
  grep -q '<string>com.menganto.investing.test</string>' "$work/home/Library/LaunchAgents/com.menganto.investing.test.plist"
check "  plist points the packs at the checkout" \
  grep -q "<string>$repo/research/packs</string>" "$work/home/Library/LaunchAgents/com.menganto.investing.test.plist"
check "  bootstrap was called with that plist" \
  grep -qF "bootstrap gui/$UID $work/home/Library/LaunchAgents/com.menganto.investing.test.plist" "$work/launchctl.log"
check "  bootstrap only after launchctl print says the old job is gone (3 lingering prints)" \
  prints_between_bootout_and_bootstrap 4
check "  bootstrap tried 3 times, launchctl's error shown" bootstrap_tried_3
check "  no previous release: points to the RUNBOOK first-install note, not --rollback" hint_is_runbook

# A second install (an older release is current): the hint is --rollback.
reset_install 18799
seed_current old1
run_install never
check "with a previous release: the failure hint is --rollback" hint_is_rollback
check "  previous is the release that was current" same "$(release_link "$work/inst" previous)" old1

# `previous` cannot be set (here: a read-only directory in its place) after `current` moved: the
# install goes on with the new release, and the EXIT trap must not delete what `current` points at.
reset_install 18799
seed_current old1
mkdir "$work/inst/previous"
chmod 555 "$work/inst/previous"
run_install never
chmod 755 "$work/inst/previous"
check "previous not settable: warns and keeps going" warned_previous
check "  current is the new release and it still exists" has_current_bin
check "  current is not the old release" current_not_old1

# --- the default env file: $HOME/.secrets/investing-app.env --------------------------------------
# Staging label, INVESTING_ENV_FILE unset, HOME a temp dir. The account's home (fake id -P) is another
# temp dir, so the staging guard compares against temp "live" paths too.
default_env="$work/home/.secrets/investing-app.env"
out_has() { grep -qF "$1" "$work/out"; }
write_env() { # <file> <INVESTING_DATA_DIR>
  mkdir -p "$(dirname "$1")"
  printf 'INVESTING_APP_TOKEN=x\nPORTFOLIO_SPREADSHEET_ID=x\nINVESTING_BIND=127.0.0.1:18799\nINVESTING_DATA_DIR=%s\n' "$2" >"$1"
}

reset_install
write_env "$default_env" "$work/inst-data"
UNSET_ENV_FILE=1 run_install never
check "INVESTING_ENV_FILE unset: the plist names \$HOME/.secrets/investing-app.env" \
  grep -qF "<string>$default_env</string>" "$work/home/Library/LaunchAgents/com.menganto.investing.test.plist"
check "  runs on to launchctl bootstrap (fake, fails)" out_has "launchctl bootstrap failed"

reset_install
UNSET_ENV_FILE=1 run_install never
check "INVESTING_ENV_FILE unset and no file there: refused, names the new path" \
  refused_with "$default_env"

# The staging guard refuses the live install root, env file and data dir under the account's home
# even when HOME points elsewhere; nothing is created there.
live_path_refused() { # <message part> [NAME=value ...]
  local want=$1
  shift
  run_install never "$@"
  [ "$(cat "$work/rc")" != 0 ] && out_has "needs its own $want" && [ ! -e "$work/inst" ] && no_plist
}
reset_install 18799
check "staging with HOME elsewhere: the live env file is refused" \
  live_path_refused INVESTING_ENV_FILE "INVESTING_ENV_FILE=$work/livehome/.secrets/investing-app.env"
reset_install 18799
check "staging with HOME elsewhere: the live install root is refused" \
  live_path_refused INVESTING_INSTALL_ROOT "INVESTING_INSTALL_ROOT=$work/livehome/.local/opt/investing-app"
reset_install
write_env "$ienv" "$work/livehome/investing-app-data"
check "staging with HOME elsewhere: the live data dir in the env file is refused" \
  live_path_refused INVESTING_DATA_DIR
reset_install
write_env "$default_env" "$work/inst-data"
UNSET_ENV_FILE=1 run_install never FAKE_LIVE_HOME="$work/home"
check "staging with HOME = the live home and INVESTING_ENV_FILE unset is refused, as before" \
  refused_with "needs its own INVESTING_ENV_FILE"

# --- id -P cannot name the account's home ---------------------------------------------------------
# install.sh must not stop silently (set -e with pipefail): it warns once and the guard compares with
# the paths under $HOME, as before id -P was used. Every mode: staging, the live label, --rollback.
id_warned() { [ "$(grep -c '^WARN: cannot read the account home (id -P)' "$work/out")" = 1 ]; }
warned_and_ran_on() { id_warned && out_has "launchctl bootstrap failed"; }
warned_and_rollback_refused() { id_warned && refused_with "missing; run ./deploy/install.sh first"; }
id_fallback_refuses() { # <FAKE_ID_MODE>
  reset_install
  write_env "$default_env" "$work/inst-data"
  UNSET_ENV_FILE=1 run_install never FAKE_ID_MODE="$1"
  refused_with "needs its own INVESTING_ENV_FILE" && id_warned && [ ! -e "$work/inst" ]
}
for mode in fail empty garbage; do
  check "id -P $mode: one WARN, the guard uses \$HOME (the default env file is refused)" \
    id_fallback_refuses "$mode"
done
reset_install 18799
run_install never FAKE_ID_MODE=fail
check "id -P fails, staging with its own paths: WARN, runs on to launchctl bootstrap (fake, fails)" \
  warned_and_ran_on
reset_install 18799
run_install never FAKE_ID_MODE=fail INVESTING_LABEL=com.menganto.investing
check "id -P fails, the live label (temp paths): WARN, runs on to launchctl bootstrap (fake, fails)" \
  warned_and_ran_on
reset_install 18799
INSTALL_ARG=--rollback run_install never FAKE_ID_MODE=fail
check "id -P fails, --rollback: WARN, then its own refusal (no plist installed yet)" \
  warned_and_rollback_refused

# --- the guard compares physical paths ------------------------------------------------------------
check "path_canon folds //, /./ and .. in a path that does not exist" \
  same "$(path_canon "$work//no/./such/../path/")" "$work/no/path"
mkdir -p "$work/pc/real"
ln -s real "$work/pc/dirlink"
ln -s real/new.env "$work/pc/dangling.env"
check "path_canon resolves a symlinked directory" same "$(path_canon "$work/pc/dirlink/x.env")" "$work/pc/real/x.env"
check "path_canon follows a symlink at the end, even a dangling one" \
  same "$(path_canon "$work/pc/dangling.env")" "$work/pc/real/new.env"
check "path_canon of / is /" same "$(path_canon /)" "/"

# A live home that exists, with links into it: other spellings of the live paths are refused too,
# and nothing under the live home changes.
lh="$work/livehome"
make_livehome() {
  mkdir -p "$lh/.local/opt/investing-app" "$lh/investing-app-data" "$work/links"
  write_env "$lh/.secrets/investing-app.env" "$lh/investing-app-data"
  ln -s ../livehome "$work/links/home"
  ln -s ../livehome/.secrets/investing-app.env "$work/links/live.env"
  ln -s ../livehome/.local/opt/investing-app "$work/links/root"
  livehome_state >"$work/livehome.before"
}
livehome_state() { find "$lh" -exec stat -f '%Lp %z %N' {} + | sort; cat "$lh/.secrets/investing-app.env"; }
other_spelling_refused() { # <NAME> <INVESTING_DATA_DIR in $ienv> [NAME=value ...]
  local name=$1 data=$2
  shift 2
  reset_install
  make_livehome
  write_env "$ienv" "$data"
  run_install never "$@"
  refused_with "needs its own $name" && [ ! -e "$work/inst" ] && no_plist && livehome_state | cmp -s - "$work/livehome.before"
}
own="$work/inst-data"
check "live env file as \$LIVE_HOME//.secrets/…: refused" \
  other_spelling_refused INVESTING_ENV_FILE "$own" "INVESTING_ENV_FILE=$lh//.secrets/investing-app.env"
check "live env file as \$LIVE_HOME/./.secrets/…: refused" \
  other_spelling_refused INVESTING_ENV_FILE "$own" "INVESTING_ENV_FILE=$lh/./.secrets/investing-app.env"
check "live env file through a symlinked directory: refused" \
  other_spelling_refused INVESTING_ENV_FILE "$own" "INVESTING_ENV_FILE=$work/links/home/.secrets/investing-app.env"
check "live env file through a symlink to the file: refused" \
  other_spelling_refused INVESTING_ENV_FILE "$own" "INVESTING_ENV_FILE=$work/links/live.env"
check "live install root through a symlink: refused" \
  other_spelling_refused INVESTING_INSTALL_ROOT "$own" "INVESTING_INSTALL_ROOT=$work/links/root"
check "live install root with a trailing /: refused" \
  other_spelling_refused INVESTING_INSTALL_ROOT "$own" "INVESTING_INSTALL_ROOT=$lh/.local/opt/investing-app/"
check "live data dir in the env file as \$LIVE_HOME/x/../investing-app-data: refused" \
  other_spelling_refused INVESTING_DATA_DIR "$lh/x/../investing-app-data"
check "live data dir in the env file through a symlinked directory: refused" \
  other_spelling_refused INVESTING_DATA_DIR "$work/links/home/investing-app-data"
check "the account home itself through a symlink (live side not canonical): refused" \
  other_spelling_refused INVESTING_ENV_FILE "$own" "FAKE_LIVE_HOME=$work/links/home" "INVESTING_ENV_FILE=$lh/.secrets/investing-app.env"
reset_install 18799
check "live home not there yet, env file as \$LIVE_HOME//.secrets/…: refused" \
  live_path_refused INVESTING_ENV_FILE "INVESTING_ENV_FILE=$lh//.secrets/investing-app.env"

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
