#!/usr/bin/env bash
set -euo pipefail
umask 077

if [[ "${1:-}" == "--help" ]]; then
  printf 'Usage: %s [pairing-file.json] [--existing]\n' "$0"
  printf 'Default: fresh Downloads/liquidlane-pairing.json; create a local testnet node and background services.\n'
  printf 'For --existing, run from the existing connector configuration folder.\n'
  exit 0
fi
if (( $# > 2 )) || [[ -n "${2:-}" && "${2:-}" != "--existing" ]]; then
  printf 'Use --help for usage.\n' >&2
  exit 2
fi
liquidlane_pairing="${1:-$HOME/Downloads/liquidlane-pairing.json}"
if [[ ! -f "$liquidlane_pairing" ]]; then
  printf 'Download fresh connector settings from Supply Liquidity first. File not found: %s\n' "$liquidlane_pairing" >&2
  exit 1
fi
liquidlane_pairing="$(realpath -- "$liquidlane_pairing")"
liquidlane_core="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
printf 'Building the connector from this checkout. Keys and funds stay on this machine.\n'
cargo build --locked --manifest-path "$liquidlane_core/Cargo.toml" --target-dir "$liquidlane_core/target" --bin liquidlane-connector
liquidlane_args=(setup "$liquidlane_pairing" --background)
if [[ "${2:-}" != "--existing" ]]; then
  liquidlane_args+=(--new-node)
fi
exec "$liquidlane_core/target/debug/liquidlane-connector" "${liquidlane_args[@]}"
