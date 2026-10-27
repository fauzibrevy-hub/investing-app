#!/bin/bash
# Install the weekly XBRL scraper job (Riset) as a user LaunchAgent. Idempotent: re-running
# re-renders the plist and reloads the job.
#
#   ./deploy/install-scraper.sh
#
# Run once, by the owner, at cutover (plan.md §3.5 step 7). Not part of install.sh: until then
# the finance-app scraper owns the scraper checkout, and two scrapers would share it.
set -euo pipefail

APP_ROOT="${INVESTING_APP_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LABEL="com.menganto.investing.scraper"
OLD_LABEL="com.menganto.finance.scraper"
PLIST_DST="$HOME/Library/LaunchAgents/$LABEL.plist"

say() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

say "preflight"
[ -f "$APP_ROOT/deploy/$LABEL.plist" ] || fail "no deploy/$LABEL.plist in $APP_ROOT"
[ -f "$APP_ROOT/deploy/scraper.sh" ] || fail "no deploy/scraper.sh in $APP_ROOT"
# `launchctl print` succeeds only while the label is loaded in this user's domain.
if launchctl print "gui/$(id -u)/$OLD_LABEL" >/dev/null 2>&1; then
  fail "$OLD_LABEL is still loaded: stop it first (plan.md §3.5 step 1); two scrapers would share one scraper checkout"
fi

say "render scraper plist"
mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
/usr/bin/sed -e "s|__HOME__|$HOME|g" -e "s|__APP_ROOT__|$APP_ROOT|g" \
  "$APP_ROOT/deploy/$LABEL.plist" > "$PLIST_DST"
chmod 644 "$PLIST_DST"
plutil -lint "$PLIST_DST" >/dev/null || fail "rendered plist is not valid: $PLIST_DST"
echo "wrote $PLIST_DST"

say "load scraper job"
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST_DST"
echo "loaded $LABEL (fires Sundays 03:00 local; manual run: launchctl kickstart gui/$(id -u)/$LABEL)"
