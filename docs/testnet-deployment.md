# Testnet Deployment

LiquidLane CKB scripts must be built as RISC-V binaries before they can be deployed as CKB code cells.

## Build Artifacts

```bash
scripts/setup-riscv-toolchain.sh
export RISCV_TOOLCHAIN_BIN=/tmp/liquidlane-riscv-toolchain/root/usr/bin
scripts/build-ckb-scripts.sh
```

The build writes stripped binaries and `ckb-scripts/build/manifest.json`. Each manifest `ckb_data_hash` is the script data hash used as the script `code_hash` after the binary is deployed in a code cell.

## Testnet Inputs Needed

Actual deployment requires a funded CKB testnet signer. Do not commit keys to the repo.

Required outside Git:

- `LIQUIDLANE_CKB_RPC_URL`: CKB testnet RPC URL.
- funded deployer key or wallet signing flow.
- enough testnet CKB to create one code cell per script and the initial vault cells.

## Core Runtime Hardening

For real settlement verification, run Core with:

```bash
LIQUIDLANE_CKB_RPC_URL=https://your-testnet-node.example \
LIQUIDLANE_REQUIRE_CKB_RPC=true \
LIQUIDLANE_CKB_ACCEPT_PENDING_TXS=false \
cargo run
```

With `LIQUIDLANE_REQUIRE_CKB_RPC=true`, supply settlement is rejected unless the configured CKB node returns the transaction as accepted. Keep pending transactions disabled for production-style flows.
