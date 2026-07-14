#!/bin/sh
set -eu

: "${FIBER_SECRET_KEY_PASSWORD:?FIBER_SECRET_KEY_PASSWORD is required}"

BASE_DIR="${FIBER_BASE_DIR:-/fiber}"
RPC_PORT="${PORT:-8227}"
P2P_PORT="${FIBER_P2P_PORT:-8228}"
RPC_LISTENING_ADDR="${RPC_LISTENING_ADDR:-0.0.0.0:${RPC_PORT}}"
CKB_RPC_URL="${FIBER_CKB_RPC_URL:-https://testnet.ckb.dev/rpc}"
LIQUIDLANE_FUNDING_BUILDER_URL="${LIQUIDLANE_CORE_FUNDING_BUILDER_URL:-${FIBER_LIQUIDLANE_FUNDING_BUILDER_URL:-}}"
if [ -z "$LIQUIDLANE_FUNDING_BUILDER_URL" ] && [ -n "${RENDER_SERVICE_ID:-}" ]; then
  LIQUIDLANE_FUNDING_BUILDER_URL="https://liquidlane-core-fiber.onrender.com/internal/fiber/funding-builder"
fi
if [ -n "$LIQUIDLANE_FUNDING_BUILDER_URL" ] && [ -z "${FIBER_FUNDING_TX_SHELL_BUILDER:-}" ]; then
  mkdir -p "$BASE_DIR"
  export LIQUIDLANE_FUNDING_BUILDER_URL
  FUNDING_PROXY="$BASE_DIR/funding-builder-proxy.sh"
  cat > "$FUNDING_PROXY" <<'SH'
#!/bin/sh
set -eu

payload="$(mktemp "${TMPDIR:-/tmp}/liquidlane-funding-builder.XXXXXX.json")"
response="$(mktemp "${TMPDIR:-/tmp}/liquidlane-funding-builder.XXXXXX.response.json")"
trap 'rm -f "$payload" "$response"' EXIT

timeout "${FIBER_FUNDING_BUILDER_STDIN_TIMEOUT_SECONDS:-3}" cat > "$payload" || true
if [ ! -s "$payload" ]; then
  echo "LiquidLane funding builder received an empty Fiber payload." >&2
  exit 1
fi

code="$(curl -sS -o "$response" -w '%{http_code}' --connect-timeout "${FIBER_FUNDING_BUILDER_CONNECT_TIMEOUT_SECONDS:-5}" --max-time "${FIBER_FUNDING_BUILDER_HTTP_TIMEOUT_SECONDS:-30}" -H 'content-type: application/json' --data-binary @"$payload" "$LIQUIDLANE_FUNDING_BUILDER_URL")"
if [ "$code" != "200" ]; then
  cat "$response" >&2
  exit 1
fi
cat "$response"
SH
  chmod +x "$FUNDING_PROXY" || true
  export FIBER_FUNDING_TX_SHELL_BUILDER="$FUNDING_PROXY"
fi
RPC_BISCUIT_PUBLIC_KEY="${FIBER_RPC_BISCUIT_PUBLIC_KEY:-${RPC_BISCUIT_PUBLIC_KEY:-}}"
RPC_AUTH_CONFIG=""
if [ -n "$RPC_BISCUIT_PUBLIC_KEY" ]; then
  RPC_AUTH_CONFIG="  biscuit_public_key: \"$RPC_BISCUIT_PUBLIC_KEY\""
fi
FUNDING_TX_SHELL_BUILDER_CONFIG=""
if [ -n "${FIBER_FUNDING_TX_SHELL_BUILDER:-}" ]; then
  FUNDING_TX_SHELL_BUILDER_CONFIG="  funding_tx_shell_builder: \"$FIBER_FUNDING_TX_SHELL_BUILDER\""
fi

mkdir -p "$BASE_DIR/ckb"
if [ -n "${FIBER_CKB_PRIVATE_KEY_B64:-}" ] && [ ! -s "$BASE_DIR/ckb/key" ]; then
  printf "%s" "$FIBER_CKB_PRIVATE_KEY_B64" | base64 -d > "$BASE_DIR/ckb/key"
elif [ -n "${FIBER_CKB_PRIVATE_KEY:-}" ] && [ ! -s "$BASE_DIR/ckb/key" ]; then
  printf "%s" "$FIBER_CKB_PRIVATE_KEY" > "$BASE_DIR/ckb/key"
elif [ ! -s "$BASE_DIR/ckb/key" ]; then
  echo "FIBER_CKB_PRIVATE_KEY_B64 is not set; generating an unfunded ephemeral testnet key."
  openssl rand -hex 32 > "$BASE_DIR/ckb/key"
fi
chmod 600 "$BASE_DIR/ckb/key" || true

cat > "$BASE_DIR/config.yml" <<EOF
fiber:
  listening_addr: "/ip4/0.0.0.0/tcp/${P2P_PORT}"
  announced_node_name: "liquidlane-testnet-render"
  bootnode_addrs:
    - "/ip4/54.179.226.154/tcp/8228/p2p/Qmes1EBD4yNo9Ywkfe6eRw9tG1nVNGLDmMud1xJMsoYFKy"
    - "/ip4/16.163.7.105/tcp/8228/p2p/QmdyQWjPtbK4NWWsvy8s69NGJaQULwgeQDT5ZpNDrTNaeV"
  announce_listening_addr: false
  chain: testnet
  scripts:
    - name: FundingLock
      script:
        code_hash: 0x6c67887fe201ee0c7853f1682c0b77c0e6214044c156c7558269390a8afa6d7c
        hash_type: type
        args: 0x
      cell_deps:
        - type_id:
            code_hash: 0x00000000000000000000000000000000000000000000000000545950455f4944
            hash_type: type
            args: 0x3cb7c0304fe53f75bb5727e2484d0beae4bd99d979813c6fc97c3cca569f10f6
        - cell_dep:
            out_point:
              tx_hash: 0x12c569a258dd9c5bd99f632bb8314b1263b90921ba31496467580d6b79dd14a7
              index: 0x0
            dep_type: code
    - name: CommitmentLock
      script:
        code_hash: 0x740dee83f87c6f309824d8fd3fbdd3c8380ee6fc9acc90b1a748438afcdf81d8
        hash_type: type
        args: 0x
      cell_deps:
        - type_id:
            code_hash: 0x00000000000000000000000000000000000000000000000000545950455f4944
            hash_type: type
            args: 0xf7e458887495cf70dd30d1543cad47dc1dfe9d874177bf19291e4db478d5751b
        - cell_dep:
            out_point:
              tx_hash: 0x12c569a258dd9c5bd99f632bb8314b1263b90921ba31496467580d6b79dd14a7
              index: 0x0
            dep_type: code

rpc:
  listening_addr: "${RPC_LISTENING_ADDR}"
${RPC_AUTH_CONFIG}

ckb:
  rpc_url: "${CKB_RPC_URL}"
${FUNDING_TX_SHELL_BUILDER_CONFIG}


services:
  - fiber
  - rpc
  - ckb
EOF

exec fnn -c "$BASE_DIR/config.yml" -d "$BASE_DIR"
