#!/bin/bash
# launchd entry point for the Investing app.
#
# Why a wrapper instead of putting everything in the plist: the plist is committed, so it must not
# carry INVESTING_APP_TOKEN. This script is the only place that reads the secret file, and it sets
# absolute defaults for every path so a wrong working directory can never turn the UI into a 404
# while /api/health still answers 200 (measured: cwd=/ gives health 200 but / 404).
#
# Where it runs from: deploy/install.sh copies this file into every release, next to the binary and
# UI it starts (<install root>/releases/<id>/{run.sh,bin/investing-app,web/dist}), and the plist runs
# <install root>/current/run.sh. `current` is resolved once, here, so the binary and the UI always
# come from the same release, and `cargo clean`, a deleted target/ or web/dist, or a rebuild in the
# git checkout cannot take the service down. The one thing still read from the checkout is the
# research packs directory, on purpose: the plist carries it (rendered by install.sh).
set -euo pipefail

ENV_FILE="${INVESTING_ENV_FILE:-$HOME/.secrets/investing-app.env}"
if [ ! -r "$ENV_FILE" ]; then
  echo "investing-app: env file not readable: $ENV_FILE" >&2
  exit 78 # EX_CONFIG
fi

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

HERE="$(cd "$(dirname "$0")" && pwd -P)"
if [ -f "$HERE/RELEASE" ]; then
  # A release directory: the normal case. The packs directory comes from the plist (the checkout
  # install.sh ran from) unless the env file sets it.
  BIN="$HERE/bin/investing-app"
  DEFAULT_WEB_DIST="$HERE/web/dist"
  DEFAULT_PACKS_DIR=""
else
  # A checkout's deploy/: a plist rendered before the install-root layout, or scripts/tools-selftest.sh.
  # Kept so that a `git pull` of this change cannot break that service's next start before the
  # first ./deploy/install.sh of the new layout re-renders the plist.
  APP_ROOT="$(dirname "$HERE")"
  BIN="$APP_ROOT/target/release/investing-app"
  DEFAULT_WEB_DIST="$APP_ROOT/web/dist"
  DEFAULT_PACKS_DIR="$APP_ROOT/research/packs"
fi

: "${INVESTING_BIND:=127.0.0.1:8789}"
: "${INVESTING_WEB_DIST:=$DEFAULT_WEB_DIST}"
: "${INVESTING_DATA_DIR:=$HOME/investing-app-data}"
# The packs default in src/config.rs is the relative `research/packs`; launchd starts this with cwd=/,
# so without an absolute path the service silently serves only its embedded copies (L4 parity:
# /api/research/packs files 0, 7 embedded_ids, against files 7 on finance-app).
: "${INVESTING_RESEARCH_PACKS_DIR:=$DEFAULT_PACKS_DIR}"
if [ -z "$INVESTING_RESEARCH_PACKS_DIR" ]; then
  echo "investing-app: INVESTING_RESEARCH_PACKS_DIR not set (plist or env file): only embedded packs" >&2
  unset INVESTING_RESEARCH_PACKS_DIR
else
  export INVESTING_RESEARCH_PACKS_DIR
fi
export INVESTING_BIND INVESTING_WEB_DIST INVESTING_DATA_DIR

if [ ! -x "$BIN" ]; then
  echo "investing-app: binary missing or not executable: $BIN" >&2
  exit 78
fi

exec "$BIN"
