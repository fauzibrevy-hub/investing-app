#!/bin/bash
# Install or update the Investing app as a user LaunchAgent. Idempotent: safe to re-run.
#
#   ./deploy/install.sh               # build, install a new release, (re)load com.menganto.investing
#   ./deploy/install.sh --rollback    # current -> previous release, restart, same health checks
#
# What it does, in order: preflight (incl. the port) -> build binary -> build UI -> copy both into
# a new release under the install root -> data dir -> render the plist to a temp file -> port check
# again -> point `current` at the new release and move the plist into place -> (re)load service
# -> health check (the listener must be the new release) -> prune old releases. A failure before
# the switch leaves `current`, the plist in ~/Library/LaunchAgents and the running job as they were
# (the staged release is removed).
#
# The service runs from the install root (INVESTING_INSTALL_ROOT, default ~/.local/opt/investing-app),
# never from this checkout: once installed, `cargo clean`, a deleted target/ or web/dist, or a
# rebuild here does not affect it. Only research/packs is still read from the checkout this ran
# from (rendered into the plist). Layout and helpers: deploy/release-lib.sh.
#
# What it does not do: write or edit the env file (the owner writes it, plan.md §3.4 step 1),
# seed data, or install the weekly scraper job (deploy/install-scraper.sh, once, at cutover).
#
# Spare instance next to the live one (RUNBOOK §2.1): INVESTING_LABEL plus its own
# INVESTING_INSTALL_ROOT and INVESTING_ENV_FILE, whose INVESTING_DATA_DIR and INVESTING_BIND must
# differ from the live ones (refused otherwise).
set -euo pipefail

say() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
warn() { printf 'WARN: %s\n' "$*" >&2; }

MODE=install
case "${1:-}" in
  "") ;;
  --rollback) MODE=rollback ;;
  -h | --help) sed -n '2,24p' "$0"; exit 0 ;;
  *) fail "unknown argument: $1 (use --rollback or nothing)" ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source-path=SCRIPTDIR source=release-lib.sh
. "$SCRIPT_DIR/release-lib.sh"

DEFAULT_LABEL="com.menganto.investing"
DEFAULT_INSTALL_ROOT="$HOME/.local/opt/investing-app"
DEFAULT_ENV_FILE="$HOME/.hermes/secrets/investing-app.env"
DEFAULT_DATA_DIR="$HOME/investing-app-data"
DEFAULT_BIND="127.0.0.1:8789"

APP_ROOT="${INVESTING_APP_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
LABEL="${INVESTING_LABEL:-$DEFAULT_LABEL}"
INSTALL_ROOT="${INVESTING_INSTALL_ROOT:-$DEFAULT_INSTALL_ROOT}"
ENV_FILE="${INVESTING_ENV_FILE:-$DEFAULT_ENV_FILE}"
DATA_DIR="${INVESTING_DATA_DIR:-$DEFAULT_DATA_DIR}"
SKIP_BUILD="${SKIP_BUILD:-0}"
KEEP_RELEASES=3
BIN_NAME="investing-app"
DOMAIN="gui/$(id -u)"

# Only this app's labels: any other label would overwrite and boot out another job's plist
# (com.menganto.finance, …). The weekly scraper job's label is not this service.
case "$LABEL" in
  "$DEFAULT_LABEL") ;;
  "$DEFAULT_LABEL.scraper") fail "INVESTING_LABEL $LABEL is the scraper job (deploy/install-scraper.sh)" ;;
  "$DEFAULT_LABEL." | "$DEFAULT_LABEL".*[!A-Za-z0-9._-]*)
    fail "INVESTING_LABEL: only letters, digits, '.', '_' and '-' after $DEFAULT_LABEL.: $LABEL" ;;
  "$DEFAULT_LABEL".*) ;;
  *) fail "INVESTING_LABEL must be $DEFAULT_LABEL or $DEFAULT_LABEL.<name>, not $LABEL" ;;
esac

PLIST_DST="$HOME/Library/LaunchAgents/$LABEL.plist"
PLIST_TMP=""
STAGED="" # a release copied but not yet current; removed when the run fails before the switch
on_exit() {
  [ -z "$PLIST_TMP" ] || rm -f "$PLIST_TMP"
  # Never the release `current` points at, whatever step failed.
  if [ -n "$STAGED" ] && [ "$(release_link "$INSTALL_ROOT" current)" != "${STAGED##*/}" ]; then
    rm -rf "$STAGED"
    echo "removed the staged release; current and $PLIST_DST are unchanged" >&2
  fi
}
trap on_exit EXIT
if [ "$LABEL" = "$DEFAULT_LABEL" ]; then LOG_NAME="investing-app"; else LOG_NAME="$LABEL"; fi
ERR_LOG="$HOME/Library/Logs/$LOG_NAME.err.log"

# env_value <KEY>: the env file's value (quotes stripped, ~/ expanded), or nothing. Never printed.
env_value() {
  local v=""
  [ -f "$ENV_FILE" ] && v="$(grep -E "^$1=" "$ENV_FILE" | head -1 | cut -d= -f2- | sed -e 's/^"//' -e 's/"$//' || true)"
  case "$v" in \~/*) v="$HOME/${v#\~/}" ;; esac
  printf '%s\n' "$v"
}

# The bind run.sh will use: the env file's INVESTING_BIND when present, else the default.
run_bind() {
  local b
  b="$(env_value INVESTING_BIND)"
  printf '%s\n' "${b:-$DEFAULT_BIND}"
}

case "$INSTALL_ROOT" in
  /*) ;;
  *) fail "INVESTING_INSTALL_ROOT must be an absolute path: $INSTALL_ROOT" ;;
esac
# A spare instance must not share anything with the live one: the default install root's `current`
# is what the live job starts, the default env file holds the live token, and run.sh falls back to
# the live data dir and port when the env file does not name its own.
if [ "$LABEL" != "$DEFAULT_LABEL" ]; then
  [ "$INSTALL_ROOT" != "$DEFAULT_INSTALL_ROOT" ] || fail "$LABEL needs its own INVESTING_INSTALL_ROOT"
  [ "$ENV_FILE" != "$DEFAULT_ENV_FILE" ] || fail "$LABEL needs its own INVESTING_ENV_FILE"
  spare_data="$(env_value INVESTING_DATA_DIR)"
  [ -n "$spare_data" ] && [ "${spare_data%/}" != "$DEFAULT_DATA_DIR" ] ||
    fail "$LABEL needs its own INVESTING_DATA_DIR in $ENV_FILE"
  [ "$(run_bind)" != "$DEFAULT_BIND" ] || fail "$LABEL needs its own INVESTING_BIND in $ENV_FILE"
fi

# Fail when another process (not this label's job) listens on the port: the new job could not bind.
check_port_free() {
  local port="$1" own p
  own="$(launchctl print "$DOMAIN/$LABEL" 2>/dev/null | awk '/^\tpid = /{print $3}' || true)" # empty when not loaded
  for p in $(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | sort -u); do
    [ "$p" = "$own" ] || fail "port $port is held by pid $p, not by $LABEL; free it or set another INVESTING_BIND in $ENV_FILE"
  done
}

# What to do when the new release is current but the job is not up. With no previous release (the
# first install with the install root) --rollback refuses, so it names the RUNBOOK way back.
recover_hint() {
  if [ -n "$(release_link "$INSTALL_ROOT" previous)" ]; then
    printf 'roll back: ./deploy/install.sh --rollback, with the same INVESTING_* variables'
  else
    printf 'no previous release to roll back to: retry launchctl bootstrap %s %s, or go back as in RUNBOOK §1 "First install with the install root"' "$DOMAIN" "$PLIST_DST"
  fi
}

# launchctl bootout returns before launchd has removed the job, and a bootstrap right after it can
# fail ("Bootstrap failed: 5: Input/output error", seen once on staging). Wait up to 10 s for the
# label to go; then try bootstrap three times, 1 s apart, showing launchctl's error each time.
unload_job() {
  launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
  for _ in $(seq 1 20); do
    launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1 || return 0
    sleep 0.5
  done
  echo "WARN: $LABEL is still loaded 10 s after bootout" >&2
}
load_job() {
  local try err
  for try in 1 2 3; do
    if err="$(launchctl bootstrap "$DOMAIN" "$PLIST_DST" 2>&1)"; then return 0; fi
    echo "launchctl bootstrap (try $try of 3): ${err:-no error text}" >&2
    [ "$try" = 3 ] || sleep 1
  done
  return 1
}

# Wait until /api/health answers 200 from a process started from <release dir>, then require / 200.
# The trap the / check exists for: a wrong UI path serves /api/health fine and 404s the UI.
wait_healthy() {
  local rel bind base port want ok=0 pid="" cmd="" code
  rel="$(cd "$1" && pwd -P)"
  bind="$(run_bind)"
  base="http://${bind%%:*}:${bind##*:}"
  port="${bind##*:}"
  want="$rel/bin/$BIN_NAME"
  for _ in $(seq 1 60); do
    if curl -sf -m 2 "$base/api/health" >/dev/null 2>&1; then
      pid="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null | head -1 || true)"
      cmd="$(ps -o command= -p "${pid:-0}" 2>/dev/null || true)"
      case "$cmd" in "$want" | "$want "*) ok=1; break ;; esac
    fi
    sleep 0.5
  done
  [ "$ok" = "1" ] || fail "no healthy $BIN_NAME from $rel at $base (listener: ${cmd:-none}; see $ERR_LOG); $(recover_hint)"
  code="$(curl -s -o /dev/null -w '%{http_code}' -m 5 "$base/")"
  [ "$code" = "200" ] || fail "UI is not served at $base/ (HTTP $code); check INVESTING_WEB_DIST in $ENV_FILE"
  echo "OK  $base/api/health 200"
  echo "OK  $base/ 200 (UI served)"
  echo "OK  pid $pid runs $want"
}

show_links() {
  echo "current  -> $(release_link "$INSTALL_ROOT" current)"
  echo "previous -> $(release_link "$INSTALL_ROOT" previous)"
}

if [ "$MODE" = "rollback" ]; then
  say "rollback $LABEL in $INSTALL_ROOT"
  [ -f "$PLIST_DST" ] || fail "$PLIST_DST missing; run ./deploy/install.sh first"
  # Restart a running job; start a stopped one (§5 rollbacks stop the service for the data step
  # first, and the new release must not start again before the old one does). A stopped job's
  # port is checked before `current` moves.
  LOADED=0
  launchctl print "$DOMAIN/$LABEL" >/dev/null 2>&1 && LOADED=1
  if [ "$LOADED" = "0" ]; then
    BIND="$(run_bind)"
    check_port_free "${BIND##*:}"
  fi
  release_rollback "$INSTALL_ROOT" "$BIN_NAME" || fail "rollback refused; current is unchanged"
  show_links
  if [ "$LOADED" = "1" ]; then
    launchctl kickstart -k "$DOMAIN/$LABEL"
  else
    load_job || fail "launchctl bootstrap failed; $(recover_hint)"
  fi
  wait_healthy "$INSTALL_ROOT/current"
  exit 0
fi

say "preflight"
[ -d "$APP_ROOT" ] || fail "app root not found: $APP_ROOT"
# Both halves are needed: the Rust crate and the built UI live in different trees, and a branch
# that carries only one of them fails here instead of at the end of the build.
[ -f "$APP_ROOT/Cargo.toml" ] || fail "no Cargo.toml in $APP_ROOT — is this a backend branch?"
[ -f "$APP_ROOT/web/package.json" ] || fail "no web/package.json in $APP_ROOT — is this a UI branch?"
command -v cargo >/dev/null || fail "cargo not on PATH"
command -v npm >/dev/null || fail "npm not on PATH"

# The env file holds the bearer token and the sheet id; the owner writes it. Presence checks only:
# no value is printed.
[ -f "$ENV_FILE" ] || fail "env file missing: $ENV_FILE (owner writes it: plan.md §3.4 step 1)"
chmod 600 "$ENV_FILE"
grep -q '^INVESTING_APP_TOKEN=' "$ENV_FILE" || fail "INVESTING_APP_TOKEN not set in $ENV_FILE"
grep -q '^PORTFOLIO_SPREADSHEET_ID=' "$ENV_FILE" || fail "PORTFOLIO_SPREADSHEET_ID not set in $ENV_FILE"
# run.sh serves the web/dist of the release it starts; an env-file pin would override that.
if grep -q '^INVESTING_WEB_DIST=' "$ENV_FILE"; then
  warn "INVESTING_WEB_DIST is set in $ENV_FILE and overrides the release's web/dist; remove the line unless it is meant to"
fi
# Before anything is built, copied or written: with the port held by another process the new job
# could not start, and a plist left behind would retry it at every login (RunAtLoad, KeepAlive).
BIND="$(run_bind)"
check_port_free "${BIND##*:}"
echo "port ${BIND##*:}: free or held by $LABEL"

if [ "$SKIP_BUILD" != "1" ]; then
  say "build backend (release)"
  ( cd "$APP_ROOT" && cargo build --release )

  say "build UI"
  ( cd "$APP_ROOT/web" && npm ci --include=dev --no-audit --no-fund && env -u NODE_ENV npm run build )
fi
[ -x "$APP_ROOT/target/release/$BIN_NAME" ] || fail "binary missing after build"
[ -f "$APP_ROOT/web/dist/index.html" ] || fail "web/dist/index.html missing after build"
[ -d "$APP_ROOT/research/packs" ] || fail "research/packs missing in $APP_ROOT (the service reads it from there)"

say "release"
RELEASE_ID="$(release_id "$APP_ROOT")"
SHA="$(git -C "$APP_ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
mkdir -p "$INSTALL_ROOT"
release_stage "$INSTALL_ROOT" "$RELEASE_ID" "$APP_ROOT/target/release/$BIN_NAME" \
  "$APP_ROOT/web/dist" "$APP_ROOT/deploy/run.sh" "$SHA" || fail "release not staged; current is unchanged"
STAGED="$INSTALL_ROOT/releases/$RELEASE_ID"
echo "staged $STAGED (not current yet)"

say "data directory"
mkdir -p "$DATA_DIR"

say "render plist"
# Rendered outside ~/Library/LaunchAgents; it moves there only at the switch below.
sed_value() { printf '%s' "$1" | sed -e 's/[\\|&]/\\&/g'; }
PLIST_TMP="$(mktemp "${TMPDIR:-/tmp}/$LABEL.plist.XXXXXX")"
/usr/bin/sed -e "s|__LABEL__|$(sed_value "$LABEL")|g" \
  -e "s|__HOME__|$(sed_value "$HOME")|g" \
  -e "s|__INSTALL_ROOT__|$(sed_value "$INSTALL_ROOT")|g" \
  -e "s|__ENV_FILE__|$(sed_value "$ENV_FILE")|g" \
  -e "s|__APP_ROOT__|$(sed_value "$APP_ROOT")|g" \
  -e "s|__LOG_NAME__|$(sed_value "$LOG_NAME")|g" \
  "$APP_ROOT/deploy/$DEFAULT_LABEL.plist" > "$PLIST_TMP"
chmod 644 "$PLIST_TMP"
plutil -lint "$PLIST_TMP" >/dev/null || fail "rendered plist is not valid; $PLIST_DST left as it was"

say "switch and load service"
# Again, right before the switch: the build may have taken minutes.
BIND="$(run_bind)"
check_port_free "${BIND##*:}"
ACTIVATED=0
release_activate "$INSTALL_ROOT" "$RELEASE_ID" || ACTIVATED=$?
case "$ACTIVATED" in
  0) ;;
  # `current` moved; carry on so the job runs what `current` says, but say what is off.
  2) echo "WARN: previous was not updated; --rollback would not return to the release that ran before" >&2 ;;
  *) fail "could not switch current to $RELEASE_ID" ;;
esac
STAGED=""
echo "installed $INSTALL_ROOT/releases/$RELEASE_ID"
show_links
mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
mv -f "$PLIST_TMP" "$PLIST_DST"
PLIST_TMP=""
echo "wrote $PLIST_DST"
unload_job
load_job || fail "launchctl bootstrap failed; $(recover_hint)"
echo "loaded $LABEL"

say "wait for health"
wait_healthy "$INSTALL_ROOT/releases/$RELEASE_ID"

say "prune releases (keep $KEEP_RELEASES)"
release_prune "$INSTALL_ROOT" "$KEEP_RELEASES" || fail "prune failed (the service is up)"
ls -1 "$INSTALL_ROOT/releases"

if [ "$LABEL" = "$DEFAULT_LABEL" ]; then
  BASE="http://${BIND%%:*}:${BIND##*:}"
  echo
  echo "Next: expose over Tailscale on 8445 (443 and 8443 belong to other apps):"
  echo "  tailscale serve --bg --https=8445 $BASE"
  echo "  curl -sf https://<mac>.<tailnet>.ts.net:8445/api/health"
fi
