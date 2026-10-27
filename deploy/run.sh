#!/bin/bash
# launchd entry point for the Finance app.
#
# Why a wrapper instead of putting everything in the plist: the plist is committed, so it must not
# carry FINANCE_APP_TOKEN. This script is the only place that reads the secret file, and it sets
# absolute defaults for every path so a wrong working directory can never turn the UI into a 404
# while /api/health still answers 200 (measured: cwd=/ gives health 200 but / 404).
set -euo pipefail

ENV_FILE="${FINANCE_ENV_FILE:-$HOME/.hermes/secrets/finance-app.env}"
if [ ! -r "$ENV_FILE" ]; then
  echo "finance-app: env file not readable: $ENV_FILE" >&2
  exit 78 # EX_CONFIG
fi

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${FINANCE_BIND:=127.0.0.1:8787}"
: "${FINANCE_WEB_DIST:=$APP_ROOT/web/dist}"
: "${FINANCE_DATA_DIR:=$HOME/finance-app-data}"
export FINANCE_BIND FINANCE_WEB_DIST FINANCE_DATA_DIR

BIN="$APP_ROOT/target/release/finance-app"
if [ ! -x "$BIN" ]; then
  echo "finance-app: binary missing or not executable: $BIN" >&2
  exit 78
fi

exec "$BIN"
