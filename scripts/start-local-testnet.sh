#!/usr/bin/env bash
# Resume this checkout's existing marketplace database and app on port 3000.
set -euo pipefail
umask 077

liquidlane_core="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
liquidlane_app="$(realpath -- "$liquidlane_core/../liquidlane-app")"
liquidlane_runtime="$liquidlane_core/runtime/testnet"
if [[ "${1:-}" == "--help" ]]; then
  printf 'Usage: %s\nStarts the existing testnet coordinator and app at http://localhost:3000.\n' "$0"
  printf 'Keep this terminal open. Ctrl+C stops only the app and coordinator started here.\n'
  exit 0
fi
if (( $# != 0 )); then
  printf 'Use --help for usage.\n' >&2
  exit 2
fi
if [[ ! -f "$liquidlane_runtime/core.sqlite3" ]]; then
  printf 'Existing testnet database not found. Restore it before resuming; no empty replacement was created.\n' >&2
  exit 1
fi
exec 9>"$liquidlane_runtime/.local-stack.lock"
flock -n 9 || { printf 'This local stack is already running.\n' >&2; exit 1; }
python3 - <<'PY'
import socket
for port in (3000, 18180):
    with socket.socket() as listener:
        try:
            listener.bind(('127.0.0.1', port))
        except OSError as error:
            raise SystemExit(f'Cannot listen on local port {port}: {error}. Existing processes were left running.')
PY
cargo build --locked --manifest-path "$liquidlane_core/Cargo.toml" --target-dir "$liquidlane_core/target" --bin liquidlane-core --bin liquidlane-connector
(cd -- "$liquidlane_app" && env NEXT_PUBLIC_API_BASE_URL=http://127.0.0.1:18180 NEXT_PUBLIC_CKB_NETWORK=testnet NEXT_PUBLIC_CKB_RPC_URL=https://testnet.ckb.dev/rpc npm run build)

liquidlane_children=()
cleanup() {
  if (( ${#liquidlane_children[@]} )); then
    kill "${liquidlane_children[@]}" 2>/dev/null || true
    wait "${liquidlane_children[@]}" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
(cd -- "$liquidlane_core" && exec env LIQUIDLANE_PRODUCT_MODE=marketplace LIQUIDLANE_ENV=development LIQUIDLANE_BIND_ADDR=127.0.0.1:18180 LIQUIDLANE_MARKET_ORIGIN=http://localhost:3000 LIQUIDLANE_MARKET_DB="$liquidlane_runtime/core.sqlite3" LIQUIDLANE_CKB_RPC_URL=https://testnet.ckb.dev/rpc LIQUIDLANE_FIBER_VERSION=0.9.0 "$liquidlane_core/target/debug/liquidlane-core") &
liquidlane_children+=("$!")
(cd -- "$liquidlane_app" && exec env NODE_ENV=production HOSTNAME=127.0.0.1 PORT=3000 node .next/standalone/server.js) &
liquidlane_children+=("$!")
printf 'Starting LiquidLane at http://localhost:3000 with its existing testnet database.\n'
printf 'After the services report ready, open Supply Liquidity. Provider node setup runs in a separate terminal.\n'
wait -n "${liquidlane_children[@]}"
