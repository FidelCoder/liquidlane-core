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

Deployed on CKB testnet on July 4, 2026. The first deployment was replaced because its build emitted VM-incompatible instructions. The active deployment below is the VM-safe redeploy used by the live vault.

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
