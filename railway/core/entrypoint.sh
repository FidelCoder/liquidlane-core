#!/bin/sh
set -eu

PORT="${PORT:-8080}"
export LIQUIDLANE_BIND_ADDR="${LIQUIDLANE_BIND_ADDR:-[::]:${PORT}}"
export LIQUIDLANE_DATA_PATH="${LIQUIDLANE_DATA_PATH:-/data/liquidlane-data.json}"

exec /usr/local/bin/liquidlane-core
