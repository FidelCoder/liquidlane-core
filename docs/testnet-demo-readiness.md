# Testnet Demo Readiness

LiquidLane is a CKB/Fiber-native product. The demo path must use the deployed CKB testnet scripts, the active vault cell, JoyID wallet signing, and a real Fiber RPC endpoint for channel opens.

## Backend Environment

Set these on Render or any hosted Core process:

```env
LIQUIDLANE_BIND_ADDR=0.0.0.0:10000
LIQUIDLANE_ENV=production
LIQUIDLANE_DATA_PATH=/var/data/liquidlane-data.json
LIQUIDLANE_CKB_NETWORK=testnet
LIQUIDLANE_CKB_RPC_URL=https://testnet.ckb.dev/rpc
LIQUIDLANE_REQUIRE_CKB_RPC=true
LIQUIDLANE_CKB_ACCEPT_PENDING_TXS=true
LIQUIDLANE_VAULT_ASSET=CKB
LIQUIDLANE_VAULT_CKB_ADDRESS=<active vault address>
LIQUIDLANE_VAULT_CELL_OUT_POINT=<active vault tx hash>#<index>
LIQUIDLANE_VAULT_LOCK_CODE_HASH=<testnet lock code hash>
LIQUIDLANE_VAULT_LOCK_OUT_POINT=<script deployment tx>#<index>
LIQUIDLANE_VAULT_TYPE_CODE_HASH=<testnet type code hash>
LIQUIDLANE_VAULT_TYPE_OUT_POINT=<script deployment tx>#<index>
LIQUIDLANE_LP_RECEIPT_TYPE_CODE_HASH=<testnet receipt type code hash>
LIQUIDLANE_LP_RECEIPT_TYPE_OUT_POINT=<script deployment tx>#<index>
LIQUIDLANE_REQUEST_TYPE_CODE_HASH=<testnet request type code hash>
LIQUIDLANE_REQUEST_TYPE_OUT_POINT=<script deployment tx>#<index>
LIQUIDLANE_FEE_CLAIM_TYPE_CODE_HASH=<testnet fee claim type code hash>
LIQUIDLANE_FEE_CLAIM_TYPE_OUT_POINT=<script deployment tx>#<index>
FIBER_RPC_URL=<fiber node json-rpc url>
FIBER_RPC_AUTH_TOKEN=<token if the node requires one>
```

`FIBER_RPC_URL` is the only piece that can be missing for supply, request, withdraw, and fee-claim testing. It is required before an operator can submit `open_channel`.

## Public Chain References

Use these committed records as the source of truth:

- Script deployment: `ckb-scripts/deployments/testnet-2026-07-04-a00be7fdb859.json`
- Vault deployment: `ckb-scripts/deployments/vault-testnet-2026-07-04-477be93d5587.json`
- Explorer: `https://pudge.explorer.nervos.org`

## Manual Testnet Flow

1. Start Core with the backend env above.
2. Start the app with `NEXT_PUBLIC_API_BASE_URL` pointing to Core.
3. Connect JoyID testnet wallet.
4. Open **Supply Liquidity**, enter a CKB amount, sign, dry-run, broadcast, and confirm the UI shows a transaction hash.
5. Confirm `/dashboard` shows an LP receipt with `receipt_cell_out_point` and updated available vault liquidity.
6. Open **Request Receive Capacity**, enter amount/days/Fiber peer pubkey, sign, broadcast, and confirm the request tx explorer link appears.
7. Confirm LiquidLane Core either submits the Fiber handoff automatically or shows a clear executor/Fiber RPC status on the request.
8. After a channel opens, verify fees move into earned/claimable accounting.
9. Claim fees or withdraw available liquidity from the LP receipt and confirm each action shows a transaction hash and Core settlement.

## Pre-Deploy Checks

Run these before deploying a public build:

```bash
cargo test
scripts/check-rust-line-count.sh
scripts/build-ckb-scripts.sh
```

The script build requires the RISC-V toolchain from `scripts/setup-riscv-toolchain.sh`.

## Demo Failure Rules

- No transaction hash means no value-moving CKB transaction was broadcast.
- If CKB RPC rejects dry-run or broadcast, show the source error and do not settle in Core.
- If Fiber RPC is not configured, channel open must fail clearly and must not mint a fake channel id.
- Never deploy with placeholder vault address, vault out-point, or script code hashes.
