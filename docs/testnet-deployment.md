# Testnet Deployment

LiquidLane CKB scripts must be built as RISC-V binaries before they can be deployed as CKB code cells.

## Explorer Reality

Local builds and local/devnet deployments do not have public block explorer pages unless you run a local explorer against that local chain. The local step only proves that the scripts compile and gives each script a deterministic CKB data hash.

Public confirmation requires CKB testnet deployment. LiquidLane scripts are CKB L1 code cells, so the explorer-verifiable records are CKB testnet transaction hashes, code-cell out-points, code hashes, script hashes, vault-cell out-points, and vault addresses where applicable.

Fiber channel activity is not the same thing as script deployment. Fiber is the payment-channel layer; the on-chain deployment, funding, and settlement records still resolve back to CKB testnet transactions and cells. Use the CKB testnet explorer as the source of truth for deployed LiquidLane scripts:

- CKB testnet explorer: https://pudge.explorer.nervos.org/
- CKB testnet RPC: https://testnet.ckb.dev/rpc
- Fiber network source: https://github.com/nervosnetwork/fiber

## Build Artifacts

```bash
scripts/setup-riscv-toolchain.sh
export RISCV_TOOLCHAIN_BIN=/tmp/liquidlane-riscv-toolchain/root/usr/bin
scripts/build-ckb-scripts.sh
```

The build writes stripped binaries and `ckb-scripts/build/manifest.json`. Each manifest `ckb_data_hash` is the script data hash used as the script `code_hash` after the binary is deployed in a code cell.

## Current Local Build Record

The current local build is recorded in `ckb-scripts/deployments/local-build-2026-07-03.template.json`.

It has no contract address, vault address, code-cell out-point, or transaction hash yet because no local or testnet deployment transaction has been broadcast from this repo. In CKB terms, an EVM-style contract address is not the main deployment identifier. The production record should track:

- code cell transaction hash
- code cell out-point: `tx_hash#index`
- code hash / data hash
- hash type: `data1`
- script args
- script hash for each exact vault/script instance
- vault cell out-point
- vault lock/type scripts
- CKB address, when the lock script can be encoded as an address
- explorer URL for every public testnet transaction

## Testnet Inputs Needed

Actual deployment requires a funded CKB testnet signer. Do not commit keys to the repo.

Required outside Git:

- `LIQUIDLANE_CKB_RPC_URL`: CKB testnet RPC URL.
- funded deployer key, hardware wallet, or wallet signing flow.
- deployer lock script or testnet address.
- vault admin lock script or admin testnet address.
- initial vault capacity budget for code cells and vault cells.
- enough testnet CKB to create one code cell per script and the initial vault cells.

Optional but recommended:

- `LIQUIDLANE_CKB_ACCEPT_PENDING_TXS=false` until the transaction is committed.
- a deployment record JSON copied from `ckb-scripts/deployments/testnet.template.json`.

## Deployment Record

After deployment, fill a non-secret record under `ckb-scripts/deployments/`, for example `testnet-2026-07-03.json`. Commit only public chain data:

```json
{
  "network": "ckb-testnet",
  "explorer_base_url": "https://pudge.explorer.nervos.org",
  "scripts": [
    {
      "name": "liquidlane-vault-lock",
      "deployment_tx_hash": "0x...",
      "code_cell_out_point": {
        "tx_hash": "0x...",
        "index": "0x0"
      },
      "code_hash": "0x...",
      "hash_type": "data1",
      "script_args": "0x...",
      "script_hash": "0x...",
      "explorer_url": "https://pudge.explorer.nervos.org/transaction/0x..."
    }
  ],
  "vault": {
    "vault_address": "ckt1...",
    "vault_cell_out_point": {
      "tx_hash": "0x...",
      "index": "0x..."
    },
    "vault_type_script_hash": "0x..."
  }
}
```

## Core Runtime Hardening

For real settlement verification, run Core with:

```bash
LIQUIDLANE_CKB_RPC_URL=https://testnet.ckb.dev/rpc \
LIQUIDLANE_REQUIRE_CKB_RPC=true \
LIQUIDLANE_CKB_ACCEPT_PENDING_TXS=false \
cargo run
```

With `LIQUIDLANE_REQUIRE_CKB_RPC=true`, supply settlement is rejected unless the configured CKB node returns the transaction as accepted. Keep pending transactions disabled for production-style flows.
