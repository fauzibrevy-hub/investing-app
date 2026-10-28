#!/bin/bash
# launchd entry point for the Investing app.
#
# Why a wrapper instead of putting everything in the plist: the plist is committed, so it must not
# carry INVESTING_APP_TOKEN. This script is the only place that reads the secret file, and it sets
# absolute defaults for every path so a wrong working directory can never turn the UI into a 404
# while /api/health still answers 200 (measured: cwd=/ gives health 200 but / 404).
set -euo pipefail

ENV_FILE="${INVESTING_ENV_FILE:-$HOME/.hermes/secrets/investing-app.env}"
if [ ! -r "$ENV_FILE" ]; then
  echo "investing-app: env file not readable: $ENV_FILE" >&2
  exit 78 # EX_CONFIG
fi

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
: "${INVESTING_BIND:=127.0.0.1:8789}"
: "${INVESTING_WEB_DIST:=$APP_ROOT/web/dist}"
: "${INVESTING_DATA_DIR:=$HOME/investing-app-data}"
# The packs default in src/config.rs is the relative `research/packs`; launchd starts this with cwd=/,
# so without an absolute default the service silently serves only its embedded copies (L4 parity:
# /api/research/packs files 0, 7 embedded_ids, against files 7 on finance-app).
: "${INVESTING_RESEARCH_PACKS_DIR:=$APP_ROOT/research/packs}"
export INVESTING_BIND INVESTING_WEB_DIST INVESTING_DATA_DIR INVESTING_RESEARCH_PACKS_DIR

BIN="$APP_ROOT/target/release/investing-app"
if [ ! -x "$BIN" ]; then
  echo "investing-app: binary missing or not executable: $BIN" >&2
  exit 78
fi

exec "$BIN"
