#!/bin/sh
set -eu

PORT="${PORT:-8080}"
export LIQUIDLANE_BIND_ADDR="${LIQUIDLANE_BIND_ADDR:-[::]:${PORT}}"
export LIQUIDLANE_DATA_PATH="${LIQUIDLANE_DATA_PATH:-/data/liquidlane-data.json}"
export LIQUIDLANE_MARKET_DB="${LIQUIDLANE_MARKET_DB:-/data/liquidlane-marketplace.sqlite3}"
export LIQUIDLANE_PRODUCT_MODE="${LIQUIDLANE_PRODUCT_MODE:-marketplace}"

exec /usr/local/bin/liquidlane-core
