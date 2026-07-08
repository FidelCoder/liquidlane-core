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

The current local build is recorded in `ckb-scripts/build/manifest.json`. Public testnet records are committed under `ckb-scripts/deployments/`. In CKB terms, an EVM-style contract address is not the main deployment identifier. The production record should track:

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
- `LIQUIDLANE_SPEND_PREVIOUS_SCRIPT_CELLS=true` only when replacing script cells before any live vault depends on them.
- funded deployer key, hardware wallet, or wallet signing flow.
- deployer lock script or testnet address.
- vault admin lock script or admin testnet address.
- initial vault capacity budget for code cells and vault cells.
- enough testnet CKB to create one code cell per script and the initial vault cells.

Optional but recommended:

- `LIQUIDLANE_CKB_ACCEPT_PENDING_TXS=true` for testnet browser flows; Core records mempool-accepted transactions and keeps the hash visible while CKB commits it.
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
LIQUIDLANE_CKB_ACCEPT_PENDING_TXS=true \
cargo run
```

With `LIQUIDLANE_REQUIRE_CKB_RPC=true`, supply settlement is rejected unless the configured CKB node returns the transaction as accepted. On CKB testnet, Core accepts `pending` and `proposed` transactions so the dapp can show a submitted receipt immediately after wallet broadcast; mainnet should keep committed-only settlement unless explicitly changed.

## Current Testnet Deployment

Deployed on CKB testnet on July 4, 2026. The first script deployment was replaced because its build emitted VM-incompatible instructions. The active deployment below is the VM-safe redeploy used by the live vault.

- Status: committed
- Script deployment transaction: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f`
- Script explorer: https://pudge.explorer.nervos.org/transaction/0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f
- Script record: `ckb-scripts/deployments/testnet-2026-07-04-a00be7fdb859.json`
- Vault init transaction: `0x477be93d5587b6ff040858605a0e2c440f6a2e3587fa1bd3dd139391e06b2370`
- Vault explorer: https://pudge.explorer.nervos.org/transaction/0x477be93d5587b6ff040858605a0e2c440f6a2e3587fa1bd3dd139391e06b2370
- Vault record: `ckb-scripts/deployments/vault-testnet-2026-07-04-477be93d5587.json`
- Deployer: `ckt1qyqxqf7spwqfwlqtyccswl0376fagku9el5q75f2vl`

Code-cell out-points:

- `liquidlane-vault-lock`: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f#0x0`
- `liquidlane-vault-type`: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f#0x1`
- `liquidlane-lp-receipt-type`: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f#0x2`
- `liquidlane-capacity-request-type`: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f#0x3`
- `liquidlane-fee-claim-type`: `0xa00be7fdb8598a58e8938403204e2d55ffdb2806566cbca7a71fc86d82dccb7f#0x4`

Live vault:

- Vault out-point: `0x477be93d5587b6ff040858605a0e2c440f6a2e3587fa1bd3dd139391e06b2370#0x0`
- Vault lock script hash: `0xc6056a079c618ea30ef26fdad8a9e654de34516f8795f91129c9dbaee2261a40`
- Vault type script hash: `0xe983346602e46328fa9dbff94540a37cb4ccc63420af91df908956553dc1f4c3`

The full vault address is stored in `ckb-scripts/deployments/vault-testnet-2026-07-04-477be93d5587.json`.
