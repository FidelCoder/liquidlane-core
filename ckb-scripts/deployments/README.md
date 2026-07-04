# LiquidLane Deployment Records

This folder is for public deployment records only. Never commit private keys, mnemonic phrases, wallet exports, RPC auth tokens, or unsigned/signed transactions that should remain private.

## Local Deployments

Local builds do not have public explorer pages. A local build creates RISC-V script binaries and CKB data hashes. It does not create a public transaction hash, code-cell out-point, vault cell, or vault address.

The current local artifact record is `local-build-2026-07-03.template.json`. Its `contract_address_equivalent` fields are `null` because no deployment transaction has been broadcast from this repo.

If you later run a local CKB devnet deployment, record the local transaction hashes and out-points in a `*.local.json` file. Those files are ignored by Git because local chain state is machine-specific.

## Testnet Deployments

For explorer-visible deployment, deploy the scripts to CKB testnet and fill a record copied from `testnet.template.json`.

CKB does not primarily identify deployed scripts by an EVM-style contract address. Track these identifiers instead:

- deployment transaction hash
- code cell out-point: `tx_hash#index`
- code hash / data hash
- hash type: `data1`
- script args
- script hash for the exact script instance
- vault cell out-point
- vault address, when the vault lock can be encoded as a CKB address
- explorer URL

Use the CKB testnet explorer for public confirmation:

```text
https://pudge.explorer.nervos.org/
```

Fiber channel operations are payment-channel activity. The LiquidLane script deployment itself is a CKB L1 deployment, so the public proof lives in CKB testnet transactions and cells.

## Safe Commit Rule

Commit:

- public transaction hashes
- public out-points
- public code hashes
- public script hashes
- public explorer URLs
- public vault addresses

Do not commit:

- private keys
- mnemonics
- wallet export files
- RPC credentials
- local-only deployment records
- raw signed transactions unless we intentionally want them public

## Current Testnet Deployment

Deployed on CKB testnet on July 4, 2026.

- Status: committed
- Deployment transaction: `0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463`
- Explorer: https://pudge.explorer.nervos.org/transaction/0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463
- Deployer: `ckt1qyqxqf7spwqfwlqtyccswl0376fagku9el5q75f2vl`
- Record: `ckb-scripts/deployments/testnet-2026-07-04-eedb45c8ebf8.json`

Code-cell out-points:

- `liquidlane-vault-lock`: `0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463#0x0`
- `liquidlane-vault-type`: `0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463#0x1`
- `liquidlane-lp-receipt-type`: `0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463#0x2`
- `liquidlane-capacity-request-type`: `0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463#0x3`
- `liquidlane-fee-claim-type`: `0xeedb45c8ebf8bad4b543194c423d20fa3e712860ca21420f55afc66001e00463#0x4`
