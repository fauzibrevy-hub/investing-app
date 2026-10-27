#!/bin/bash
# Install or update the Investing app as a user LaunchAgent. Idempotent: safe to re-run.
#
#   ./deploy/install.sh
#
# What it does, in order: preflight -> build binary -> build UI -> data dir -> render plist ->
# (re)load service -> health check.
#
# What it does not do: write or edit the env file (the owner writes it, plan.md §3.4 step 1),
# seed data, or install the weekly scraper job (deploy/install-scraper.sh, once, at cutover).
set -euo pipefail

APP_ROOT="${INVESTING_APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
ENV_FILE="${INVESTING_ENV_FILE:-$HOME/.hermes/secrets/investing-app.env}"
DATA_DIR="${INVESTING_DATA_DIR:-$HOME/investing-app-data}"
LABEL="com.menganto.investing"
PLIST_DST="$HOME/Library/LaunchAgents/$LABEL.plist"
SKIP_BUILD="${SKIP_BUILD:-0}"

say() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

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

# The bind run.sh will use: the env file's INVESTING_BIND when present, else the default.
BIND="$(grep -E '^INVESTING_BIND=' "$ENV_FILE" | head -1 | cut -d= -f2- | sed -e 's/^"//' -e 's/"$//' || true)"
BIND="${BIND:-127.0.0.1:8789}"

if [ "$SKIP_BUILD" != "1" ]; then
  say "build backend (release)"
  ( cd "$APP_ROOT" && cargo build --release )

  say "build UI"
  ( cd "$APP_ROOT/web" && npm ci --include=dev --no-audit --no-fund && env -u NODE_ENV npm run build )
fi
[ -x "$APP_ROOT/target/release/investing-app" ] || fail "binary missing after build"
[ -f "$APP_ROOT/web/dist/index.html" ] || fail "web/dist/index.html missing after build"

say "data directory"
mkdir -p "$DATA_DIR"

say "render plist"
mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
/usr/bin/sed -e "s|__HOME__|$HOME|g" -e "s|__APP_ROOT__|$APP_ROOT|g" \
  "$APP_ROOT/deploy/$LABEL.plist" > "$PLIST_DST"
chmod 644 "$PLIST_DST"
plutil -lint "$PLIST_DST" >/dev/null || fail "rendered plist is not valid: $PLIST_DST"
echo "wrote $PLIST_DST"

say "load service"
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST_DST"
echo "loaded $LABEL"

say "wait for health"
BASE="http://${BIND%%:*}:${BIND##*:}"
ok=0
for _ in $(seq 1 40); do
  if curl -sf -m 2 "$BASE/api/health" >/dev/null 2>&1; then ok=1; break; fi
  sleep 0.5
done
[ "$ok" = "1" ] || fail "service did not become healthy at $BASE/api/health (see $HOME/Library/Logs/investing-app.err.log)"

# The trap this check exists for: a wrong working directory serves /api/health fine and 404s the UI.
UI_CODE="$(curl -s -o /dev/null -w '%{http_code}' -m 5 "$BASE/")"
[ "$UI_CODE" = "200" ] || fail "UI is not served at $BASE/ (HTTP $UI_CODE) — check INVESTING_WEB_DIST"

echo
echo "OK  $BASE/api/health 200"
echo "OK  $BASE/ 200 (UI served)"
echo
echo "Next: expose over Tailscale on 8445 (443 and 8443 belong to other apps):"
echo "  tailscale serve --bg --https=8445 $BASE"
echo "  curl -sf https://<mac>.<tailnet>.ts.net:8445/api/health"
